//! What went wrong recently — read only when a problem report is filed.
//!
//! Nothing else in this app records failures where a human can reach them.
//! `tracing_subscriber::fmt()` in the Tauri host writes to stdout, which on
//! Android is logcat: invisible in the app and gone with the process. On the
//! frontend side a thrown exception from the CodeMirror bundle, a rejected
//! promise, or a failed `invoke` reaches the devtools console and nowhere else —
//! and nobody has devtools attached to a phone at the moment something breaks.
//! So a report said "it broke" and the user's memory was the error trail.
//!
//! This is a bounded ring of the last [`CAPACITY`] failures, attached to a
//! report by [`crate::components::feedback`].
//!
//! **Why a `thread_local!` and not a Dioxus signal.** Three of the four
//! producers fire outside any Dioxus scope — a panic hook, two JS event
//! listeners, and a patched `console.error` — where a signal write panics (the
//! same constraint `sync_refresh.rs` hit). That module bridged back in with an
//! mpsc channel because it had to wake the UI; nothing here does. The buffer is
//! read once, on demand, by a modal that is mounting anyway. wasm is
//! single-threaded, so a thread-local is effectively a global.
//!
//! **Why a panic flushes to `localStorage` and nothing else does.** wasm has no
//! unwinding: a panic traps the module and there is no running app left to open
//! the capture modal. The single most valuable entry is therefore the one an
//! in-memory ring structurally cannot deliver, so the panic hook — and only the
//! panic hook — writes the buffer out synchronously on its way down.
//! [`install`] reads that key back at next boot and clears it, so a crash trail
//! survives exactly one relaunch: long enough to be reported, not long enough to
//! accumulate. Every other producer stays memory-only, because a debounced disk
//! write per console line is the churn `screen_context.rs` was written to avoid.
//!
//! **Everything here is truncated.** Entries are capped in count and each one in
//! length, because this text is bound for a synced event. A stack trace or a
//! serialized request body is exactly how something unbounded ends up in the
//! event log.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

/// Entries kept. Deep enough to hold the cascade a single failure sets off —
/// one failed `invoke` typically logs two or three lines — and shallow enough
/// that the whole buffer is still readable in a markdown report.
const CAPACITY: usize = 50;

/// Per-entry character cap. A wasm panic message carries a full location and a
/// JS exception carries a stack; neither is worth more than its first line.
const MAX_TEXT: usize = 500;

/// Where the panic hook leaves the buffer for the next boot to find.
#[cfg(target_arch = "wasm32")]
const CRASH_KEY: &str = "omni_diag_crash";

/// What produced an entry. Kept as a closed enum rather than a free string
/// because it round-trips through `localStorage` — an unknown tag on the way
/// back in is a bug, not a new category.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Panic,
    Error,
    Warn,
    Invoke,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Panic => "panic",
            Kind::Error => "error",
            Kind::Warn => "warn",
            Kind::Invoke => "invoke",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "panic" => Some(Kind::Panic),
            "error" => Some(Kind::Error),
            "warn" => Some(Kind::Warn),
            "invoke" => Some(Kind::Invoke),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    /// Milliseconds since [`install`] ran. `None` marks an entry restored from
    /// the previous session, where an offset from a boot that no longer exists
    /// would be worse than no offset at all.
    at_ms: Option<f64>,
    kind: Kind,
    text: String,
}

thread_local! {
    static BUFFER: RefCell<VecDeque<Entry>> = const { RefCell::new(VecDeque::new()) };
    static START_MS: Cell<f64> = const { Cell::new(0.0) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Re-entrancy guard. The `console.error` patch means recording can itself
    /// log; without this, one such line recurses until the stack blows.
    static RECORDING: Cell<bool> = const { Cell::new(false) };
}

/// Milliseconds since boot, or `None` when there is no clock to ask.
///
/// `js_sys::Date::now` rather than `performance.now` deliberately: it needs no
/// extra `web-sys` feature, and the value is only ever rendered as a relative
/// offset, so wall-clock jitter is immaterial at this resolution.
fn now_ms() -> Option<f64> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(js_sys::Date::now() - START_MS.with(Cell::get))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Flatten newlines and truncate to [`MAX_TEXT`], marking that it happened.
///
/// Char-indexed rather than byte-sliced: a JS exception message can carry
/// multi-byte content, and a byte slice through one panics. Newlines go because
/// the payload renders one entry per markdown list item, and an embedded break
/// silently splits a single failure into what reads as several.
fn clamp(text: &str) -> String {
    let flat = text.replace('\n', " ⏎ ");
    if flat.chars().count() <= MAX_TEXT {
        return flat;
    }
    let head: String = flat.chars().take(MAX_TEXT).collect();
    format!("{head}…[truncated]")
}

/// Push one entry, evicting the oldest past [`CAPACITY`].
///
/// Silently does nothing if a record is already in flight or the buffer is
/// already borrowed. Losing a diagnostic line is always preferable to a
/// diagnostic subsystem taking the app down with it.
fn record(kind: Kind, text: &str) {
    if RECORDING.replace(true) {
        return;
    }
    let entry = Entry {
        at_ms: now_ms(),
        kind,
        text: clamp(text),
    };
    BUFFER.with(|b| {
        if let Ok(mut buf) = b.try_borrow_mut() {
            if buf.len() >= CAPACITY {
                buf.pop_front();
            }
            buf.push_back(entry);
        }
    });
    RECORDING.set(false);
}

/// Record a command that came back `Err`.
///
/// Called from the three `bridge.rs` IPC helpers that every backend call
/// funnels through. The command name matters as much as the message: "failed"
/// with no `cmd` cannot be told apart from the twenty other things that also
/// failed.
///
/// Genuinely unused under `mock`, where the `#[cfg(feature = "mock")]` branches
/// return canned data without reaching those helpers — the browser loop has no
/// IPC to fail. `expect` rather than `allow` so that if the mock half ever does
/// start recording, the now-unfulfilled expectation says so instead of leaving a
/// stale exemption behind.
#[cfg_attr(feature = "mock", expect(dead_code))]
pub fn record_invoke_failure(cmd: &str, err: &str) {
    record(Kind::Invoke, &format!("{cmd}: {err}"));
}

/// A boot-race invoke that failed and then succeeded on retry.
///
/// Recorded as a **warning, not a failure**: the call ultimately worked, so it
/// is not a defect the user experienced. It is kept because the retry would
/// otherwise erase the only evidence that the race still happens — and the race
/// getting slower (a bigger log, a slower device) is exactly the thing worth
/// noticing before it outruns the deadline and starts failing open again.
#[cfg_attr(feature = "mock", expect(dead_code))]
pub fn record_invoke_recovery(cmd: &str, attempts: u32, waited_ms: f64) {
    record(
        Kind::Warn,
        &format!("{cmd}: recovered after {attempts} retr(y/ies), {waited_ms:.0}ms"),
    );
}

/// The buffer as display lines, newest last — the order
/// `FeedbackCapturedPayload::recent_errors` documents.
pub fn snapshot() -> Vec<String> {
    BUFFER.with(|b| {
        b.try_borrow()
            .map(|buf| buf.iter().map(render_line).collect())
            .unwrap_or_default()
    })
}

fn render_line(e: &Entry) -> String {
    match e.at_ms {
        Some(ms) => format!("[+{:.1}s] {}: {}", ms / 1000.0, e.kind.as_str(), e.text),
        None => format!("[prev] {}: {}", e.kind.as_str(), e.text),
    }
}

// -----------------------------------------------------------------------------
// Installation — browser only
// -----------------------------------------------------------------------------

/// Install the panic hook, the two `window` listeners and the console patches,
/// and restore any crash trail the previous session left behind.
///
/// Idempotent, and called once from `main` before `dioxus::launch` so that a
/// failure during the app's own first render is already covered.
#[cfg(target_arch = "wasm32")]
pub fn install() {
    if INSTALLED.replace(true) {
        return;
    }
    START_MS.set(js_sys::Date::now());

    restore_crash_trail();
    install_panic_hook();
    install_window_listeners();
    patch_console("error", Kind::Error);
    patch_console("warn", Kind::Warn);
}

/// No-op off the browser. The crate's tests are host-native (see
/// `UI_WORKFLOW.md` § Verification), so the ring logic must build and run
/// without a JS runtime to install anything into.
#[cfg(not(target_arch = "wasm32"))]
pub fn install() {}

/// Chain a recording hook ahead of the existing one.
///
/// Chained rather than replaced: the console output is what makes a panic
/// debuggable when devtools *are* attached, and dropping it to gain the buffer
/// would be a straight trade down.
#[cfg(target_arch = "wasm32")]
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(Kind::Panic, &info.to_string());
        flush_crash_trail();
        previous(info);
    }));
}

#[cfg(target_arch = "wasm32")]
fn install_window_listeners() {
    use wasm_bindgen::prelude::*;

    let Some(window) = web_sys::window() else {
        return;
    };

    // Uncaught exceptions: the CodeMirror bundle, the Dioxus runtime, and
    // anything else in the page that throws outside our own call stack.
    let on_error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(|e: web_sys::ErrorEvent| {
        record(
            Kind::Error,
            &format!(
                "uncaught: {} ({}:{})",
                e.message(),
                e.filename(),
                e.lineno()
            ),
        );
    });
    let _ = window.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref());
    on_error.forget();

    // Rejected promises — the shape most IPC and fetch failures actually take,
    // and the one `window.onerror` does not see.
    let on_reject = Closure::<dyn FnMut(web_sys::PromiseRejectionEvent)>::new(
        |e: web_sys::PromiseRejectionEvent| {
            let reason = e
                .reason()
                .as_string()
                .unwrap_or_else(|| format!("{:?}", e.reason()));
            record(Kind::Error, &format!("unhandled rejection: {reason}"));
        },
    );
    let _ = window
        .add_event_listener_with_callback("unhandledrejection", on_reject.as_ref().unchecked_ref());
    on_reject.forget();
}

/// Wrap one `console` method so its calls are recorded as well as printed.
///
/// The wrapper is built **in JS**, not as a Rust closure bound directly.
/// `console.error` is variadic and a Rust closure cannot reach JS `arguments` —
/// binding one would silently drop every argument past the first and change what
/// the console prints. The factory below forwards `arguments` untouched to the
/// original and hands Rust a joined string.
///
/// The `try/catch` around the sink is load-bearing: a throw inside it would
/// otherwise turn every `console.error` in the app into a second failure.
#[cfg(target_arch = "wasm32")]
fn patch_console(method: &str, kind: Kind) {
    use wasm_bindgen::prelude::*;

    let Some(console) = js_sys::Reflect::get(&js_sys::global(), &"console".into())
        .ok()
        .filter(|c| c.is_object())
    else {
        return;
    };
    let Ok(original) = js_sys::Reflect::get(&console, &method.into()) else {
        return;
    };
    if !original.is_function() {
        return;
    }

    let sink = Closure::<dyn FnMut(String)>::new(move |s: String| record(kind, &s));

    let factory = js_sys::Function::new_with_args(
        "orig, sink",
        "return function () { \
           try { sink(Array.prototype.map.call(arguments, String).join(' ')); } \
           catch (e) { /* a broken sink must never break logging */ } \
           return orig.apply(this, arguments); \
         };",
    );

    if let Ok(wrapped) = factory.call2(&JsValue::NULL, &original, sink.as_ref()) {
        let _ = js_sys::Reflect::set(&console, &method.into(), &wrapped);
    }
    // Lives for the whole app session — there is exactly one per method,
    // registered at boot. Same contract as `bridge::listen_backend_event`.
    sink.forget();
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Write the buffer out as `[["kind","text"], …]`.
///
/// Offsets are dropped on purpose: a relative time from a boot that no longer
/// exists is misleading, and [`Entry::at_ms`] being `None` is what marks these
/// as belonging to the previous session on the way back in.
#[cfg(target_arch = "wasm32")]
fn flush_crash_trail() {
    let pairs: Vec<[String; 2]> = BUFFER.with(|b| {
        b.try_borrow()
            .map(|buf| {
                buf.iter()
                    .map(|e| [e.kind.as_str().to_string(), e.text.clone()])
                    .collect()
            })
            .unwrap_or_default()
    });
    if pairs.is_empty() {
        return;
    }
    if let (Some(store), Ok(json)) = (local_storage(), serde_json::to_string(&pairs)) {
        let _ = store.set_item(CRASH_KEY, &json);
    }
}

/// Read back and clear whatever the last session's panic hook left.
///
/// Cleared unconditionally, including when the parse fails: a key that cannot be
/// read is a key that would otherwise be re-attached to every report from here
/// on.
#[cfg(target_arch = "wasm32")]
fn restore_crash_trail() {
    let Some(store) = local_storage() else {
        return;
    };
    let raw = store.get_item(CRASH_KEY).ok().flatten();
    let _ = store.remove_item(CRASH_KEY);
    let Some(raw) = raw else {
        return;
    };
    let Ok(pairs) = serde_json::from_str::<Vec<[String; 2]>>(&raw) else {
        return;
    };
    BUFFER.with(|b| {
        if let Ok(mut buf) = b.try_borrow_mut() {
            for [kind, text] in pairs.into_iter().take(CAPACITY) {
                if let Some(kind) = Kind::parse(&kind) {
                    buf.push_back(Entry {
                        at_ms: None,
                        kind,
                        text,
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drain between tests: `thread_local!` state is shared across every test on
    /// the same thread, so a leftover entry would make ordering assertions pass
    /// or fail depending on test order.
    fn reset() {
        BUFFER.with(|b| b.borrow_mut().clear());
        RECORDING.set(false);
    }

    // Host-native, so `now_ms()` is `None` and every line renders with the
    // `[prev]` prefix. The offset half is browser-only and is covered by the
    // Playwright pass instead.

    #[test]
    fn evicts_oldest_past_capacity() {
        reset();
        for i in 0..CAPACITY + 10 {
            record(Kind::Error, &format!("e{i}"));
        }
        let lines = snapshot();
        assert_eq!(lines.len(), CAPACITY);
        assert!(lines[0].ends_with("e10"), "oldest survivor: {}", lines[0]);
        assert!(
            lines[CAPACITY - 1].ends_with(&format!("e{}", CAPACITY + 9)),
            "newest last: {}",
            lines[CAPACITY - 1]
        );
    }

    #[test]
    fn truncates_long_entries() {
        reset();
        record(Kind::Panic, &"x".repeat(MAX_TEXT * 3));
        let line = snapshot().remove(0);
        assert!(line.ends_with("…[truncated]"));
        assert!(line.chars().count() < MAX_TEXT + 40);
    }

    /// A multi-byte tail must not be sliced mid-character — the failure mode a
    /// byte-indexed truncation would only hit on non-ASCII error text.
    #[test]
    fn truncates_on_char_boundary() {
        reset();
        record(Kind::Error, &"é".repeat(MAX_TEXT * 2));
        let line = snapshot().remove(0);
        assert!(line.contains('é'));
        assert!(line.ends_with("…[truncated]"));
    }

    #[test]
    fn newlines_are_flattened() {
        reset();
        record(Kind::Panic, "line one\nline two");
        assert_eq!(snapshot(), vec!["[prev] panic: line one ⏎ line two"]);
    }

    #[test]
    fn invoke_failures_name_the_command() {
        reset();
        record_invoke_failure("get_workspace", "__ipc_timeout__");
        let line = snapshot().remove(0);
        assert!(
            line.contains("invoke: get_workspace: __ipc_timeout__"),
            "{line}"
        );
    }

    #[test]
    fn kind_tags_round_trip() {
        for k in [Kind::Panic, Kind::Error, Kind::Warn, Kind::Invoke] {
            assert_eq!(Kind::parse(k.as_str()), Some(k));
        }
        assert_eq!(Kind::parse("something-else"), None);
    }

    #[test]
    fn snapshot_is_empty_before_anything_fails() {
        reset();
        assert!(snapshot().is_empty());
    }
}
