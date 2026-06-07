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

    /// Snapshot the session for `key`, if one is being tracked.
    pub fn get(&self, key: &ContinuityKey) -> Option<EditSession> {
        self.sessions.read().get(key).cloned()
    }

    /// Insert or replace the session for `key`. `&self` (not `&mut self`) because
    /// `Signal` is a `Copy` interior-mutable handle — call sites needn't hold a
    /// mutable binding.
    pub fn put(&self, key: ContinuityKey, session: EditSession) {
        let mut sessions = self.sessions;
        sessions.write().insert(key, session);
    }

    /// Drop a session once it's fully persisted and no longer needs recovering.
    pub fn remove(&self, key: &ContinuityKey) {
        let mut sessions = self.sessions;
        sessions.write().remove(key);
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
    }

    /// Drop a capture draft once it's committed (saved) or abandoned (back).
    pub fn remove_capture(&self, key: &ContinuityKey) {
        let mut captures = self.captures;
        captures.write().remove(key);
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
    }

    /// Non-subscribing snapshot of the saved navigation position (1.8b) — for
    /// one-time boot restoration / page-init reads, which must not subscribe.
    pub fn nav_peek(&self) -> NavState {
        self.nav.peek().clone()
    }

    /// Mutate the saved navigation position in place. Pages call this from a
    /// write-through effect as their sub-position changes; the debounced persist
    /// effect (which subscribes to `nav`) then flushes it to disk.
    pub fn update_nav(&self, f: impl FnOnce(&mut NavState)) {
        let mut nav = self.nav;
        let mut guard = nav.write();
        f(&mut guard);
    }

    /// Snapshot all three maps for on-disk persistence (1.8a). Reads — and thus
    /// subscribes — so the debounced persist effect re-runs on any change.
    pub fn snapshot_for_persist(&self) -> PersistedWorkspace {
        PersistedWorkspace {
            sessions: self
                .sessions
                .read()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            captures: self
                .captures
                .read()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            lists: self
                .lists
                .read()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            nav: self.nav.read().clone(),
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
    // `loaded` gates the persistence writer until the boot read finishes (1.8a)
    // *and* is read by pages to gate first hydration (1.8b). It lives on the
    // struct so descendants can consult it via `loaded_peek`.
    let mut loaded = use_signal(|| false);
    let store = ContinuityStore {
        sessions,
        captures,
        lists,
        nav,
        loaded,
    };
    use_context_provider(|| store);

    // Boot: read the persisted store from disk and repopulate the maps. On a
    // read *error* we leave the writer disabled so a transient failure can't
    // overwrite a good file with an empty one.
    use_future(move || async move {
        if let Ok(json) = crate::bridge::invoke_get_workspace().await {
            if !json.is_empty()
                && let Ok(w) = serde_json::from_str::<PersistedWorkspace>(&json)
            {
                store.load_from_persist(w);
            }
            loaded.set(true);
        }
    });

    // Debounced write-back: any post-load change flushes to disk after a quiet
    // period, with a generation counter cancelling superseded writes — the same
    // cancel pattern the editors use for auto-save.
    let mut persist_gen = use_signal(|| 0u64);
    use_effect(move || {
        if !*loaded.read() {
            return;
        }
        let snapshot = store.snapshot_for_persist();
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
