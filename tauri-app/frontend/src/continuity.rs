//! Root-held continuity store (Phase 1.1).
//!
//! Per-page editing state used to die whenever a page component unmounted. The
//! shell in `main.rs` swaps pages with `match *active_tab.read()`, so switching
//! tabs *drops* the old page component — and with it `content`,
//! `last_saved_content`, and any in-flight debounced auto-save (see
//! `pages/journal.rs` `DayView` and `pages/notes.rs` `NoteEditor`). The result:
//! lost keystrokes and reset scroll position on navigation.
//!
//! This store lifts that recoverable state to the app root via
//! `use_context_provider` (joining the existing `tz` / `pending_share`
//! contexts), so it survives page unmount. Disk-level persistence — surviving an
//! Android app-kill / restart — is layered on top later (task 1.8); this module
//! is the in-memory tier only.
//!
//! The journal editor and the generic-notes editor share the same editing-
//! session shape (`EditSession`), so one `sessions` map backs both (see
//! `feedback_shared_ui_shape_is_a_tell`). Surfaces with a different shape get
//! their own parallel map keyed by the same `ContinuityKey`: `captures` holds
//! in-flight finances capture drafts (`CaptureDraft`, task 1.4); transaction-
//! list pagination state (task 1.5) will be added the same way.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use dioxus::prelude::*;

/// One editing session's *recoverable* state — the slice that must outlive an
/// unmount.
///
/// - `content`: the live editor buffer (what the user has typed).
/// - `last_saved_content`: mirror of what's persisted to the backend; auto-save
///   diffs `content` against it to decide whether a save is needed.
/// - `save_generation`: monotonic counter so a newer keystroke cancels an older
///   pending debounced save (each scheduled save bails if this has moved on).
/// - `title`: the note title. Used by the generic-notes editor (which has a
///   title field); the journal editor leaves it empty — journal entries are
///   keyed by date, not titled.
/// - `cursor`: char offset of the selection head when the page was last left
///   (1.8b). Restored into CodeMirror on remount so returning to a note drops
///   the caret — and, via `scrollIntoView`, the viewport — back where it was.
///
/// Transient UI (loading / error / llm-result / "Saving…" flags) deliberately
/// stays page-local — losing it on unmount is harmless and re-derives on remount.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct EditSession {
    pub title: String,
    pub content: String,
    pub last_saved_content: String,
    pub save_generation: u64,
    /// `#[serde(default)]` so a pre-1.8b on-disk blob (no `cursor` key) still
    /// deserializes — it just restores at offset 0.
    #[serde(default)]
    pub cursor: usize,
}

/// `ContinuityKey` — the identity that addresses one entry in
/// the store.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContinuityKey {
    Journal(String),
    Note(String),
    NewNote,
    Capture(String),
    TxnList(String),
}

/// One in-flight finances capture (task 1.4): the editable `TransactionForm`
/// draft, held so a tab switch (which unmounts `FinancesPage`) can't lose a
/// half-confirmed receipt. Fields mirror the form but use primitive types so
/// this foundational module needn't import the page-local `PostingRow`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CaptureDraft {
    pub date: String,
    pub description: String,
    pub postings: Vec<PostingDraft>,
    pub attachment: Option<crate::types::AttachmentRef>,
}

impl CaptureDraft {
    /// A draft worth resuming has *some* user-meaningful content. An untouched
    /// blank manual form is not — it shouldn't raise the "resume capture"
    /// affordance on Home or linger in the store.
    pub fn is_empty(&self) -> bool {
        self.description.trim().is_empty()
            && self.attachment.is_none()
            && self
                .postings
                .iter()
                .all(|p| p.account.trim().is_empty() && p.amount.trim().is_empty())
    }
}

/// One posting row inside a `CaptureDraft` — account / commodity / amount, all
/// staged as strings exactly as the form holds them mid-edit.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct PostingDraft {
    pub account: String,
    pub commodity: String,
    pub amount: String,
}

/// Transaction-list pagination state (task 1.5): the loaded rows, how far we've
/// paged, whether more remain, and the active filter — held so navigating to a
/// transaction's detail and back restores the list (rows + scroll + filter)
/// instead of snapping back to a freshly-fetched page 0.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ListState {
    pub transactions: Vec<crate::types::TransactionView>,
    pub offset: u32,
    pub has_more: bool,
    pub filter: crate::types::TxnFilter,
}

/// Restorable navigation position (1.8b): which top-level tab the user last had
/// open, plus each feature's sub-position, so a boot — or an Android app-kill —
/// returns them where they were instead of the default Journal/Today.
///
/// Stored as plain strings (not the page-local `Tab` / view enums) so this
/// foundational module stays dependency-free, mirroring `CaptureDraft`'s
/// primitive typing. Each page owns the string⇆enum mapping at its boundary.
/// Every field is optional: a fresh install (or a pre-1.8b on-disk blob) leaves
/// them `None`, and each page falls back to its own default.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct NavState {
    /// Top-level tab key: "journal" | "notes" | "routines" | "finances" | "settings".
    pub tab: Option<String>,
    /// Journal: the selected day (`YYYY-MM-DD`).
    pub journal_date: Option<String>,
    /// Journal sub-tab: "today" | "calendar".
    pub journal_subtab: Option<String>,
    /// Notes view: "list" | "new" | "edit".
    pub notes_view: Option<String>,
    /// Notes: the open note's id when `notes_view == "edit"`.
    pub notes_edit_id: Option<String>,
    /// Notes sub-tab: "recent" | "search".
    pub notes_subtab: Option<String>,
    /// Finances surface: "overview" | "ledger" | "analyze" (Stage C sub-nav).
    #[serde(default)]
    pub finances_view: Option<String>,
}

/// Debounce before flushing the store to disk (1.8a). A touch longer than the
/// editor auto-save debounce so a burst of edits batches into one write.
const PERSIST_DEBOUNCE_MS: i32 = 1500;

/// On-disk shape of the whole continuity store (1.8a). The live store keys its
/// maps by `ContinuityKey` (an enum); `serde_json` can't use a non-string map
/// key, so each map persists as a `Vec<(key, value)>`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct PersistedWorkspace {
    pub sessions: Vec<(ContinuityKey, EditSession)>,
    pub captures: Vec<(ContinuityKey, CaptureDraft)>,
    pub lists: Vec<(ContinuityKey, ListState)>,
    /// `#[serde(default)]` so a pre-1.8b blob (no `nav` key) still loads.
    #[serde(default)]
    pub nav: NavState,
}

/// Does this session hold anything a reader would act on?
///
/// **The one predicate**, shared by the two hydrate paths (`journal.rs`,
/// `notes.rs`) and by [`ContinuityStore::put`]'s eviction. Deliberately one
/// function rather than three copies of `content != last_saved_content`: the
/// readers discard clean sessions and `put` therefore declines to store them,
/// and those two behaviours are only safe together. If a reader started
/// accepting clean sessions while `put` kept dropping them, the store would
/// silently lose live data — and the bug would look like a sync fault, not a
/// caching one. Sharing the function makes that divergence unrepresentable.
///
/// "Clean" means saved: `last_saved_content` is what the backend last
/// acknowledged, so equality means the backend copy is at least as fresh, and
/// the fresh copy must win — otherwise a session stored at first open
/// permanently shadows edits synced in from another device.
pub fn session_is_recoverable(session: &EditSession) -> bool {
    session.content != session.last_saved_content
}

/// Root continuity store. Cheap to copy (it's a handle to a `Signal`); one
/// instance is provided at the app root and read by every page via context.
#[derive(Clone, Copy)]
pub struct ContinuityStore {
    sessions: Signal<HashMap<ContinuityKey, EditSession>>,
    captures: Signal<HashMap<ContinuityKey, CaptureDraft>>,
    lists: Signal<HashMap<ContinuityKey, ListState>>,
    /// Last navigation position (1.8b), restored at boot.
    nav: Signal<NavState>,
    /// Flips true once the boot disk-read finishes (1.8a/1.8b). Pages gate their
    /// first hydration on this so the initially-open page sees a disk-restored
    /// session instead of racing the load and falling back to the backend copy.
    loaded: Signal<bool>,
    /// Ephemeral read-cache for finances surfaces (Stage C3). Keyed by a caller
    /// string (e.g. `"ov:dash"`, `"nw:6m"`); holds the last-fetched payload as
    /// JSON so a revisit renders instantly (stale-while-revalidate) instead of
    /// flashing a loader while it re-queries. Deliberately **not** persisted —
    /// it's always re-derivable from the backend, so it stays out of
    /// `PersistedWorkspace` to keep the on-disk workspace lean.
    reads: Signal<HashMap<String, serde_json::Value>>,
    /// Bumped by every mutation of the three persisted maps (and `nav`). The
    /// debounced write-back subscribes to *this* rather than to the maps
    /// themselves, so a keystroke costs one `u64` increment instead of a deep
    /// clone of the whole workspace. See `snapshot_for_persist`.
    ///
    /// Not persisted, and not a version number anyone reads — only its
    /// *changing* matters.
    revision: Signal<u64>,
}

impl ContinuityStore {
    /// Non-subscribing read of the boot-load flag — for the page hydration gate,
    /// which polls inside an async load future and must not subscribe.
    pub fn loaded_peek(&self) -> bool {
        *self.loaded.peek()
    }

    /// Subscribing read of the boot-load flag — for the nav-restore effects,
    /// which must re-run when the disk snapshot finishes loading (a page can
    /// mount before that happens).
    pub fn is_loaded(&self) -> bool {
        *self.loaded.read()
    }

    /// Mark the persisted state as changed, waking the debounced write-back.
    ///
    /// Every mutator of `sessions` / `captures` / `lists` / `nav` must call this
    /// — a mutation that skips it is a change that never reaches disk, which on
    /// Android (where the OS kills the app without warning) is exactly the data
    /// loss the continuity store exists to prevent.
    fn bump(&self) {
        let mut revision = self.revision;
        *revision.write() += 1;
    }

    /// Snapshot the session for `key`, if one is being tracked.
    pub fn get(&self, key: &ContinuityKey) -> Option<EditSession> {
        self.sessions.read().get(key).cloned()
    }

    /// Insert or replace the session for `key` — **unless it is clean**, in
    /// which case any existing session for that key is dropped instead.
    ///
    /// `&self` (not `&mut self`) because `Signal` is a `Copy` interior-mutable
    /// handle — call sites needn't hold a mutable binding.
    ///
    /// **Why clean sessions are dropped rather than stored.** Both readers —
    /// `journal.rs` and `notes.rs`, the only two in the app — hydrate with
    /// `store.get(&key).filter(|s| s.content != s.last_saved_content)`. A clean
    /// session is therefore *already* discarded at every read, deliberately: it
    /// must yield to the fresh backend copy or it permanently shadows edits that
    /// synced in from another device. Storing one buys nothing and costs a great
    /// deal, because a session is inserted the first time a day or note is
    /// **opened** — read-only, no edits — and `remove` was wired for exactly one
    /// transient key (`NewNote`). In a daily journalling app that is one
    /// full-text entry per day accumulating for the life of the install, each
    /// one re-serialised into the persisted workspace blob and re-parsed at
    /// every cold start.
    ///
    /// So the eviction predicate is not a new retention policy to tune — it is
    /// the readers' own filter, applied at write time. Behaviour is unchanged by
    /// construction: nothing can observe a session that every reader rejects.
    ///
    /// Enforced here rather than at the three call sites for the same reason
    /// `box_request` exists: a rule at the call site is a rule that the fourth
    /// writer forgets.
    pub fn put(&self, key: ContinuityKey, session: EditSession) {
        let mut sessions = self.sessions;
        if !session_is_recoverable(&session) {
            sessions.write().remove(&key);
            self.bump();
            return;
        }
        sessions.write().insert(key, session);
        self.bump();
    }

    /// Drop a session once it's fully persisted and no longer needs recovering.
    pub fn remove(&self, key: &ContinuityKey) {
        let mut sessions = self.sessions;
        sessions.write().remove(key);
        self.bump();
    }

    /// Snapshot the in-flight capture draft for `key`, if one is tracked.
    /// Subscribes the caller — use for reactive reads (e.g. the Home "resume
    /// capture" affordance, which must update when a capture appears or clears).
    pub fn get_capture(&self, key: &ContinuityKey) -> Option<CaptureDraft> {
        self.captures.read().get(key).cloned()
    }

    /// Non-subscribing read of the capture draft — for one-time hydration in a
    /// render body, where subscribing would re-render on every write-through.
    pub fn peek_capture(&self, key: &ContinuityKey) -> Option<CaptureDraft> {
        self.captures.peek().get(key).cloned()
    }

    /// Insert or replace the in-flight capture draft for `key`.
    pub fn put_capture(&self, key: ContinuityKey, draft: CaptureDraft) {
        let mut captures = self.captures;
        captures.write().insert(key, draft);
        self.bump();
    }

    /// Drop a capture draft once it's committed (saved) or abandoned (back).
    pub fn remove_capture(&self, key: &ContinuityKey) {
        let mut captures = self.captures;
        captures.write().remove(key);
        self.bump();
    }

    /// Non-subscribing read of a list's pagination state — for one-time
    /// hydration in a render body (see `peek_capture` for the why).
    pub fn peek_list(&self, key: &ContinuityKey) -> Option<ListState> {
        self.lists.peek().get(key).cloned()
    }

    /// Insert or replace a list's pagination state.
    pub fn put_list(&self, key: ContinuityKey, state: ListState) {
        let mut lists = self.lists;
        lists.write().insert(key, state);
        self.bump();
    }

    /// Non-subscribing snapshot of the saved navigation position (1.8b) — for
    /// one-time boot restoration / page-init reads, which must not subscribe.
    pub fn nav_peek(&self) -> NavState {
        self.nav.peek().clone()
    }

    /// Mutate the saved navigation position in place. Pages call this from a
    /// write-through effect as their sub-position changes; the debounced persist
    /// effect (which subscribes to `revision`) then flushes it to disk.
    pub fn update_nav(&self, f: impl FnOnce(&mut NavState)) {
        let mut nav = self.nav;
        {
            let mut guard = nav.write();
            f(&mut guard);
        }
        // After the guard drops: `bump` takes its own write lock.
        self.bump();
    }

    /// Non-subscribing read of a cached finances payload (Stage C3), deserialized
    /// to `T`. `peek` — a surface hydrates its signal from this at init without
    /// subscribing (the fetch effect, not the cache, drives re-renders).
    pub fn cache_get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.reads
            .peek()
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Write a freshly-fetched finances payload into the read-cache (Stage C3).
    /// Called after each successful fetch so the next revisit renders instantly.
    pub fn cache_put<T: serde::Serialize>(&self, key: &str, val: &T) {
        if let Ok(v) = serde_json::to_value(val) {
            let mut reads = self.reads;
            reads.write().insert(key.to_string(), v);
        }
    }

    /// Snapshot all three maps for on-disk persistence (1.8a).
    ///
    /// **`peek`, not `read`** — this deliberately does *not* subscribe. It used
    /// to, which was how the persist effect knew to re-run; the effect now
    /// subscribes to the cheap `revision` counter instead and calls this only
    /// after its debounce has elapsed. Subscribing here would drag the deep
    /// clone back onto the per-change path, which is the whole thing being
    /// fixed: this clones every session, capture and — via `ListState` — the
    /// entire accumulated Ledger list with each row's `postings` JSON. Only the
    /// `to_string` + disk write were ever debounced, so typing one character in
    /// today's journal entry deep-cloned all of that, on every keystroke.
    pub fn snapshot_for_persist(&self) -> PersistedWorkspace {
        PersistedWorkspace {
            sessions: self
                .sessions
                .peek()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            captures: self
                .captures
                .peek()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            lists: self
                .lists
                .peek()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            nav: self.nav.peek().clone(),
        }
    }

    /// Replace all maps + nav from a persisted snapshot (boot rehydrate, 1.8a/1.8b).
    pub fn load_from_persist(&self, w: PersistedWorkspace) {
        let mut sessions = self.sessions;
        let mut captures = self.captures;
        let mut lists = self.lists;
        let mut nav = self.nav;
        *sessions.write() = w.sessions.into_iter().collect();
        *captures.write() = w.captures.into_iter().collect();
        *lists.write() = w.lists.into_iter().collect();
        *nav.write() = w.nav;
    }
}

/// Create the store and provide it at the app root. Call exactly once, in `App`,
/// next to the existing `use_context_provider` calls.
pub fn use_continuity_provider() -> ContinuityStore {
    let sessions = use_signal(HashMap::<ContinuityKey, EditSession>::new);
    let captures = use_signal(HashMap::<ContinuityKey, CaptureDraft>::new);
    let lists = use_signal(HashMap::<ContinuityKey, ListState>::new);
    let nav = use_signal(NavState::default);
    let reads = use_signal(HashMap::<String, serde_json::Value>::new);
    let revision = use_signal(|| 0u64);
    // `loaded` gates the persistence writer until the boot read finishes (1.8a)
    // *and* is read by pages to gate first hydration (1.8b). It lives on the
    // struct so descendants can consult it via `loaded_peek`.
    let mut loaded = use_signal(|| false);
    // Separate from `loaded`: true only once the boot read actually *succeeded*.
    // `loaded` means "boot read finished, pages may hydrate" (set even on failure
    // so a transient early-invoke error can't strand every `loaded_peek` waiter on
    // "Loading…" forever); `load_succeeded` gates the write-back so a failed read
    // still can't clobber a good on-disk file with an empty snapshot.
    let mut load_succeeded = use_signal(|| false);
    let store = ContinuityStore {
        sessions,
        captures,
        lists,
        nav,
        loaded,
        reads,
        revision,
    };
    use_context_provider(|| store);

    // Boot: read the persisted store from disk and repopulate the maps. On a
    // read *error* we leave the writer disabled so a transient failure can't
    // overwrite a good file with an empty one.
    use_future(move || async move {
        // Boot read with *timeout-bounded* retry (cold-start readiness race). On a
        // fresh/empty DB the backend `setup` runs a slow `init_all()` *before* it
        // `manage`s AppState. A very early `get_workspace` invoke can hit unmanaged
        // state and return Err — but on genuine first-open (Android) it can also be
        // **dropped** by the not-yet-ready IPC so its promise NEVER settles. The
        // previous retry-on-Err loop awaited the invoke directly, so a dropped
        // invoke parked here forever: `loaded` never flipped and every
        // `loaded_peek` waiter (journal fetch, `main.rs` tab-restore) hung on
        // "Loading…" indefinitely (the fresh-install cold-open hang, root-caused
        // on-device 2026-07-05 — the shell + wasm actually render in ~85ms; only
        // this boot read stalled). `invoke_get_workspace_timed` races each attempt
        // against a `setTimeout`, so a hung attempt fails like an Err and we retry.
        // Deadline is a wall-clock fail-open cap covering both failure modes (fast
        // Err spin and silent hang) so a broken backend degrades to an empty
        // session instead of a dead UI.
        const ATTEMPT_TIMEOUT_MS: i32 = 500;
        const RETRY_GAP_MS: i32 = 100;
        const DEADLINE_MS: i32 = 15_000; // setup finishes in <1s; generous fail-open
        let mut spent = 0i32;
        loop {
            match crate::bridge::invoke_get_workspace_timed(ATTEMPT_TIMEOUT_MS).await {
                Ok(json) => {
                    if !json.is_empty()
                        && let Ok(w) = serde_json::from_str::<PersistedWorkspace>(&json)
                    {
                        store.load_from_persist(w);
                    }
                    load_succeeded.set(true);
                    break;
                }
                Err(_) => {
                    // Upper-bound this attempt's cost (a hung attempt burns the full
                    // timeout; a fast Err burns ~0 but we still count it so the
                    // deadline stays conservative), then gap before retrying.
                    spent += ATTEMPT_TIMEOUT_MS + RETRY_GAP_MS;
                    if spent >= DEADLINE_MS {
                        break;
                    }
                    crate::timer::sleep_ms(RETRY_GAP_MS).await;
                }
            }
        }
        loaded.set(true);
    });

    // Debounced write-back: any post-load change flushes to disk after a quiet
    // period, with a generation counter cancelling superseded writes — the same
    // cancel pattern the editors use for auto-save.
    let mut persist_gen = use_signal(|| 0u64);
    use_effect(move || {
        // Gate on load *success*, not merely load *finished*: if the boot read
        // failed we must not flush an empty snapshot over a good on-disk file.
        if !*load_succeeded.read() {
            return;
        }
        // Subscribe to the revision counter, *not* to the maps. Reading the maps
        // here (which is what calling `snapshot_for_persist` used to do) both
        // subscribed the effect and performed the deep clone, so every keystroke
        // paid a full workspace copy while only the serialize + disk write were
        // debounced.
        let _ = store.revision.read();
        let scheduled = {
            let mut g = persist_gen.write();
            *g += 1;
            *g
        };
        spawn(async move {
            crate::timer::sleep_ms(PERSIST_DEBOUNCE_MS).await;
            if *persist_gen.peek() != scheduled {
                return;
            }
            // Snapshot *after* the debounce and after the gen check, so a burst
            // of keystrokes yields exactly one clone rather than one per key.
            // Taken here it is also fresher than a pre-debounce copy would be.
            let snapshot = store.snapshot_for_persist();
            if let Ok(json) = serde_json::to_string(&snapshot) {
                let _ = crate::bridge::invoke_save_workspace(&json).await;
            }
        });
    });

    store
}

/// Read the continuity store from any descendant page.
pub fn use_continuity() -> ContinuityStore {
    use_context::<ContinuityStore>()
}
