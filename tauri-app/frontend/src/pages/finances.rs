use dioxus::prelude::*;

use crate::bridge;
use crate::continuity::{use_continuity, CaptureDraft, ContinuityKey, ListState, PostingDraft};
use crate::types::{
    AccountSummaryView, AffordVerdictView, AttachmentRef, BalanceCheckView, BudgetProgress,
    BudgetRow, DashboardSummaryView, DraftTransactionView, ExtractedDraft, JournalImportPlan,
    JournalImportPreview, JournalImportResult, MatchCandidateView, MonthlyTrendBucketView,
    PendingBatchView, PendingShareCapture, PostingInput, ReconciliationTxnPreview,
    RecurringObligationView, RecurringPattern, ScanRecurringResult, TransactionFormDraft,
    TransactionView, TxnFilter,
};

/// Which kind of file-based capture the user opened. Drives the picker
/// `accept` filter, the camera hint, the title, and whether the hint
/// selector is offered (PDFs require a user pick; photos default to receipt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentKind {
    Photo,
    Pdf,
}

/// Classify a `PendingShareCapture` MIME into a `DocumentKind`. Used when an
/// Android share-target intent hands us bytes and we need to decide which
/// capture view (Photo vs PDF) to route into.
///
/// Returns `None` when the MIME is something we don't support as a financial
/// document (e.g., `text/plain` — that flow lives in EmailCapture, not here).
/// `None` short-circuits the share-target routing: the bytes are dropped and
/// the user lands on the regular Finances home so they can pick a flow
/// manually.
///
/// `filename` is included so the implementation can fall back to the file
/// extension when the MIME is generic (Android share apps sometimes hand us
/// `application/octet-stream` for a PDF).
///
/// **Permissive by design.** Anything in `application/*` with a `.pdf` name
/// (legacy `application/x-pdf`, misdeclared `application/zip`, the canonical
/// `application/octet-stream` stripped-MIME case) routes to Pdf. The looseness
/// is safe because Gemini is the actual validator downstream — a wrongly-
/// classified share fails the extraction round-trip with a clean error that
/// surfaces in `CaptureState::Error` one Retry away from recovery. Tightening
/// this classifier costs lost legitimate shares; loosening it costs one
/// recoverable round-trip. Asymmetry favors permissiveness.
fn classify_share_mime(mime: &str, filename: &str) -> Option<DocumentKind> {
    const IMAGE_SUBTYPES: &[&str] = &["jpeg", "jpg", "png", "heic", "heif", "webp"];
    let mime = mime.to_ascii_lowercase();
    if let Some(subtype) = mime.strip_prefix("image/")
        && IMAGE_SUBTYPES.contains(&subtype)
    {
        return Some(DocumentKind::Photo);
    }
    if let Some(subtype) = mime.strip_prefix("application/")
        && (subtype == "pdf" || filename.to_ascii_lowercase().ends_with(".pdf"))
    {
        return Some(DocumentKind::Pdf);
    }
    None
}

/// Which sub-view is currently active inside Finances. The variants stay
/// `Copy`; the pending extracted draft (which is not Copy) lives in a
/// separate signal alongside this enum.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FinancesView {
    Home,
    Capture(DocumentKind),
    Email,
    /// Editable confirm-draft / manual-entry form. The initial state comes
    /// from `pending_draft` — `None` is manual entry, `Some(_)` is the
    /// post-extraction confirm step.
    TransactionForm,
    /// List of pending auto-import batches awaiting review (Phase 3.10.6).
    BatchList,
    /// Per-batch review screen. Selected `batch_id` is carried in the
    /// `selected_batch_id` signal alongside the view enum (the enum stays
    /// `Copy` so this variant can't carry the String inline).
    BatchReview,
    /// Committed-transactions browse screen (Phase 4.1).
    TransactionList,
    /// Single-transaction detail screen with attachment viewer (Phase 4.2).
    /// Selected `txn_id` rides along in `selected_txn_id` (variant stays Copy).
    TransactionDetail,
    /// Accounts screen — per-account balances aggregated to base currency
    /// (Phase 4.4). One card per declared+listable account.
    AccountList,
    /// R1 financial-health glance dashboard (Phase 4.5 + 4.6). Net worth,
    /// Unmatched, monthly trend, recurring, can-I-afford.
    Dashboard,
    /// W4 budget setup screen (Phase 5.1). Per-category targets with
    /// per-cycle (monthly default; weekly / biweekly) cadence.
    BudgetList,
    /// W3 recurring confirm/dismiss screen (Phase 5.4). Lists patterns
    /// surfaced by the scanner, each accept/dismiss per row.
    RecurringReview,
    /// CIBC chequing CSV import screen (Phase 5.5). Each parsed row emits
    /// a `TransactionRecorded` with one source-account posting + one
    /// `Unmatched` placeholder, awaiting 5.7 reconciliation pairing.
    StatementImport,
    /// Unified reconciliation review (Phase 5.7). Two-column candidate
    /// pairs from the matching engine (5.6); accept merges, dismiss
    /// skips. Reachable from Home + the dashboard's Unmatched widget.
    Reconciliation,
    /// Balance-check form (Phase 5.8). Compares sum of cleared
    /// transactions on an account to a user-supplied statement closing
    /// balance; flags discrepancy.
    BalanceCheck,
    /// hledger journal import (Phase 6.2 + 6.3). User picks a path,
    /// previews per-account stats, optionally drops/renames accounts,
    /// and commits as a batch of TransactionRecorded events.
    JournalImport,
    /// R2 ad-hoc query builder (Phase 7.1 + 7.2). Compose field predicates into
    /// a filter DSL, evaluate it host-side, and browse the matching transactions.
    Query,
}

/// Top-level Finances page. Umbrella for capture flows (Phase 3), transactions
/// surface (Phase 4), workflows (Phase 5), and import (Phase 6).
#[component]
pub fn FinancesPage() -> Element {
    let store = use_continuity();
    let mut view = use_signal(|| FinancesView::Home);
    let mut pending_draft: Signal<Option<ExtractedDraft>> = use_signal(|| None);
    let mut selected_batch_id: Signal<Option<String>> = use_signal(|| None);
    let mut selected_txn_id: Signal<Option<String>> = use_signal(|| None);
    // Dashboard widget click-through can seed the next TransactionList
    // render with a pre-applied filter (e.g. account: "Unmatched"). One-shot
    // — the list reads + clears it on mount.
    let mut pending_txn_filter: Signal<Option<TxnFilter>> = use_signal(|| None);
    // Pending-batch count, refreshed every time the user lands on Home. A
    // separate signal (not derived from listing the batches inline) keeps the
    // Home banner cheap — one COUNT query instead of a full SELECT every
    // navigation.
    let mut pending_batch_count: Signal<u64> = use_signal(|| 0);
    let _refresh_count_resource = use_resource(move || {
        let in_home = matches!(*view.read(), FinancesView::Home);
        async move {
            if !in_home {
                return;
            }
            if let Ok(batches) = bridge::invoke_list_pending_batches().await {
                pending_batch_count.set(batches.len() as u64);
            }
        }
    });

    // Pending Android share-target intake (Phase 3.3). main.rs sets this
    // signal when MainActivity.kt stashes shared bytes; we route to the
    // matching capture view, hand the bytes to DocumentCapture as a
    // `preloaded` prop, and clear the signal so a Back-then-forward navigation
    // doesn't replay the same share.
    let mut pending_share: Signal<Option<PendingShareCapture>> = use_context();
    use_effect(move || {
        // Snapshot + drop the read guard before any .set() — Dioxus signals
        // hold the read borrow until end-of-expression, so set/read in the
        // same statement deadlocks the borrow checker.
        let snapshot = pending_share.read().clone();
        let Some(capture) = snapshot else { return };
        match classify_share_mime(&capture.mime, &capture.filename) {
            Some(kind) => view.set(FinancesView::Capture(kind)),
            None => {
                // Unsupported MIME (e.g., text/html share) — drop the bytes;
                // user lands on Home and can pick a flow manually.
                pending_share.set(None);
            }
        }
    });

    // In-flight capture (1.4): if a half-finished TransactionForm draft is
    // stashed in the store, surface a "resume" affordance on Home. Only read
    // (and thus subscribe to) the capture store while actually on Home — this
    // keeps form keystrokes, which write the store on every change, from
    // re-rendering the whole FinancesPage. `filter(is_empty)` is
    // belt-and-suspenders; the form's mirror already drops empty drafts.
    let pending_capture = if matches!(*view.read(), FinancesView::Home) {
        store
            .get_capture(&active_capture_key())
            .filter(|d| !d.is_empty())
    } else {
        None
    };
    let pending_capture_label = pending_capture.as_ref().map(|d| {
        let desc = d.description.trim();
        if desc.is_empty() {
            "Untitled capture".to_string()
        } else {
            desc.to_string()
        }
    });

    rsx! {
        div { class: "max-w-3xl mx-auto w-full animate-in fade-in duration-300",

            match *view.read() {
                FinancesView::Home => rsx! {
                    HomeView {
                        pending_count: *pending_batch_count.read(),
                        has_pending_capture: pending_capture.is_some(),
                        pending_capture_label: pending_capture_label.clone(),
                        on_resume_capture: move |_| view.set(FinancesView::TransactionForm),
                        on_open_photo: move |_| view.set(FinancesView::Capture(DocumentKind::Photo)),
                        on_open_pdf: move |_| view.set(FinancesView::Capture(DocumentKind::Pdf)),
                        on_open_email: move |_| view.set(FinancesView::Email),
                        on_open_manual: move |_| {
                            // Manual entry is a fresh blank form: clear any stale
                            // in-flight capture so the form starts empty instead
                            // of resuming it (Resume is the explicit path).
                            store.remove_capture(&active_capture_key());
                            pending_draft.set(None);
                            view.set(FinancesView::TransactionForm);
                        },
                        on_open_batches: move |_| view.set(FinancesView::BatchList),
                        on_open_transactions: move |_| view.set(FinancesView::TransactionList),
                        on_open_accounts: move |_| view.set(FinancesView::AccountList),
                        on_open_dashboard: move |_| view.set(FinancesView::Dashboard),
                        on_open_budgets: move |_| view.set(FinancesView::BudgetList),
                        on_open_recurring: move |_| view.set(FinancesView::RecurringReview),
                        on_open_statement_import: move |_| view.set(FinancesView::StatementImport),
                        on_open_reconciliation: move |_| view.set(FinancesView::Reconciliation),
                        on_open_balance_check: move |_| view.set(FinancesView::BalanceCheck),
                        on_open_journal_import: move |_| view.set(FinancesView::JournalImport),
                        on_open_query: move |_| view.set(FinancesView::Query),
                    }
                },
                FinancesView::Capture(kind) => rsx! {
                    DocumentCapture {
                        kind: kind,
                        preloaded: pending_share.read().clone(),
                        on_done: move |_| {
                            pending_share.set(None);
                            view.set(FinancesView::Home);
                        },
                        on_extracted: move |draft: ExtractedDraft| {
                            pending_share.set(None);
                            pending_draft.set(Some(draft));
                            view.set(FinancesView::TransactionForm);
                        },
                    }
                },
                FinancesView::Email => rsx! {
                    EmailCapture {
                        on_done: move |_| view.set(FinancesView::Home),
                        on_extracted: move |draft: ExtractedDraft| {
                            pending_draft.set(Some(draft));
                            view.set(FinancesView::TransactionForm);
                        },
                    }
                },
                FinancesView::TransactionForm => rsx! {
                    TransactionForm {
                        initial: pending_draft.read().clone(),
                        on_done: move |_| {
                            pending_draft.set(None);
                            view.set(FinancesView::Home);
                        },
                    }
                },
                FinancesView::BatchList => rsx! {
                    BatchListView {
                        on_back: move |_| view.set(FinancesView::Home),
                        on_open_batch: move |batch_id: String| {
                            selected_batch_id.set(Some(batch_id));
                            view.set(FinancesView::BatchReview);
                        },
                    }
                },
                FinancesView::BatchReview => {
                    let bid = selected_batch_id.read().clone();
                    match bid {
                        Some(batch_id) => rsx! {
                            BatchReviewView {
                                batch_id: batch_id,
                                on_done: move |_| {
                                    selected_batch_id.set(None);
                                    view.set(FinancesView::BatchList);
                                },
                            }
                        },
                        None => rsx! {
                            // Shouldn't happen — BatchReview is only set
                            // alongside selected_batch_id, but if it does the
                            // user can navigate back to the list cleanly.
                            div {
                                class: "text-obsidian-text-muted text-sm",
                                "No batch selected. "
                                button {
                                    class: "underline",
                                    onclick: move |_| view.set(FinancesView::BatchList),
                                    "Back to list"
                                }
                            }
                        },
                    }
                }
                FinancesView::TransactionList => {
                    // Drain the one-shot filter seed (set by dashboard
                    // widget click-throughs). Snapshot + clear before render
                    // so a back-and-forth doesn't re-apply it.
                    let seed = pending_txn_filter.read().clone();
                    if seed.is_some() {
                        pending_txn_filter.set(None);
                    }
                    rsx! {
                        TransactionListView {
                            on_back: move |_| view.set(FinancesView::Home),
                            on_open_txn: move |txn_id: String| {
                                selected_txn_id.set(Some(txn_id));
                                view.set(FinancesView::TransactionDetail);
                            },
                            initial_filter: seed,
                        }
                    }
                },
                FinancesView::TransactionDetail => {
                    let tid = selected_txn_id.read().clone();
                    match tid {
                        Some(txn_id) => rsx! {
                            TransactionDetailView {
                                txn_id: txn_id,
                                on_back: move |_| {
                                    selected_txn_id.set(None);
                                    view.set(FinancesView::TransactionList);
                                },
                            }
                        },
                        None => rsx! {
                            div {
                                class: "text-obsidian-text-muted text-sm",
                                "No transaction selected. "
                                button {
                                    class: "underline",
                                    onclick: move |_| view.set(FinancesView::TransactionList),
                                    "Back to list"
                                }
                            }
                        },
                    }
                }
                FinancesView::AccountList => rsx! {
                    AccountListView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::Dashboard => rsx! {
                    DashboardView {
                        on_back: move |_| view.set(FinancesView::Home),
                        on_open_unmatched: move |_| {
                            // Per 4.5 spec — Unmatched widget click-through
                            // lands the user in 5.7's reconciliation review
                            // (now that 5.7 has shipped).
                            view.set(FinancesView::Reconciliation);
                        },
                    }
                },
                FinancesView::BudgetList => rsx! {
                    BudgetListView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::RecurringReview => rsx! {
                    RecurringReviewView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::StatementImport => rsx! {
                    StatementImportView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::Reconciliation => rsx! {
                    ReconciliationReviewView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::BalanceCheck => rsx! {
                    BalanceCheckFormView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::JournalImport => rsx! {
                    JournalImportView {
                        on_back: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::Query => rsx! {
                    QueryBuilderView {
                        on_back: move |_| view.set(FinancesView::Home),
                        on_open_txn: move |txn_id: String| {
                            selected_txn_id.set(Some(txn_id));
                            view.set(FinancesView::TransactionDetail);
                        },
                    }
                },
            }
        }
    }
}

#[component]
fn HomeView(
    pending_count: u64,
    has_pending_capture: bool,
    pending_capture_label: Option<String>,
    on_resume_capture: EventHandler<()>,
    on_open_photo: EventHandler<()>,
    on_open_pdf: EventHandler<()>,
    on_open_email: EventHandler<()>,
    on_open_manual: EventHandler<()>,
    on_open_batches: EventHandler<()>,
    on_open_transactions: EventHandler<()>,
    on_open_accounts: EventHandler<()>,
    on_open_dashboard: EventHandler<()>,
    on_open_budgets: EventHandler<()>,
    on_open_recurring: EventHandler<()>,
    on_open_statement_import: EventHandler<()>,
    on_open_reconciliation: EventHandler<()>,
    on_open_balance_check: EventHandler<()>,
    on_open_journal_import: EventHandler<()>,
    on_open_query: EventHandler<()>,
) -> Element {
    rsx! {
        h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent mb-8", "Finances" }

        if pending_count > 0 {
            button {
                class: "w-full mb-6 px-4 py-3 bg-obsidian-accent/10 border border-obsidian-accent/40 rounded-lg flex items-center justify-between hover:bg-obsidian-accent/15 transition-colors",
                onclick: move |_| on_open_batches.call(()),
                div { class: "flex items-center gap-3",
                    span { class: "inline-flex items-center justify-center w-8 h-8 bg-obsidian-accent text-black rounded-full text-sm font-bold",
                        "{pending_count}"
                    }
                    div { class: "text-left",
                        div { class: "text-sm font-semibold text-obsidian-text",
                            if pending_count == 1 {
                                "1 auto-imported batch awaiting review"
                            } else {
                                "{pending_count} auto-imported batches awaiting review"
                            }
                        }
                        div { class: "text-xs text-obsidian-text-muted",
                            "Tap to accept, skip, or dismiss."
                        }
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
        }

        // --- Resume in-flight capture (1.4) ---
        if has_pending_capture {
            button {
                class: "w-full mb-6 px-4 py-3 bg-amber-500/10 border border-amber-500/40 rounded-lg flex items-center justify-between hover:bg-amber-500/15 transition-colors",
                onclick: move |_| on_resume_capture.call(()),
                div { class: "flex items-center gap-3 min-w-0",
                    span { class: "inline-flex items-center justify-center w-8 h-8 bg-amber-500/20 text-amber-400 rounded-full shrink-0",
                        svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                                d: "M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
                            }
                        }
                    }
                    div { class: "text-left min-w-0",
                        div { class: "text-sm font-semibold text-obsidian-text", "Resume capture in progress" }
                        if let Some(label) = &pending_capture_label {
                            div { class: "text-xs text-obsidian-text-muted truncate", "{label}" }
                        }
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted shrink-0",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
        }

        // --- Capture Section ---
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "Capture a Transaction" }
                p { class: "text-xs text-obsidian-text-muted mt-1",
                    "Snap a receipt, drop a statement, paste an email, or enter manually."
                }
            }

            div { class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                CaptureTile {
                    label: "Photo",
                    icon_path: "M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z M15 13a3 3 0 11-6 0 3 3 0 016 0z",
                    enabled: true,
                    on_click: move |_| on_open_photo.call(()),
                }
                CaptureTile {
                    label: "PDF",
                    icon_path: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z",
                    enabled: true,
                    on_click: move |_| on_open_pdf.call(()),
                }
                CaptureTile {
                    label: "Email",
                    icon_path: "M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z",
                    enabled: true,
                    on_click: move |_| on_open_email.call(()),
                }
                CaptureTile {
                    label: "Manual",
                    icon_path: "M12 4v16m8-8H4",
                    enabled: true,
                    on_click: move |_| on_open_manual.call(()),
                }
            }
        }

        // --- Recent / glance section ---
        div { class: "border-b border-white/5 pb-2 mb-4",
            h2 { class: "text-lg font-bold text-obsidian-text", "Glance + browse" }
        }
        div { class: "space-y-3",
            button {
                class: "w-full p-4 bg-obsidian-accent/10 border border-obsidian-accent/30 rounded-lg flex items-center justify-between hover:bg-obsidian-accent/15 hover:border-obsidian-accent/50 transition-colors text-left",
                onclick: move |_| on_open_dashboard.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-accent", "Dashboard" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Net worth, trend, recurring, and can-I-afford."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_transactions.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "View transactions" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Browse everything you've recorded."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_query.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Query transactions" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Build a filter — account, tag, date, amount — and run it."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_accounts.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Accounts" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Balances per account, aggregated to your base currency."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
        }

        // --- Plan + reconcile section (Phase 5). 5.4 confirm-recurring +
        // 5.7 reconciliation review will add their own cards alongside Budgets
        // as those screens land.
        div { class: "border-b border-white/5 pb-2 mt-10 mb-4",
            h2 { class: "text-lg font-bold text-obsidian-text", "Plan + reconcile" }
        }
        div { class: "space-y-3",
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_budgets.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Budgets" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Set per-category targets; weekly, biweekly, or monthly."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_recurring.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Recurring" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Review detected subscription patterns; accept or dismiss each."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_statement_import.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Import statement" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Drop a CIBC chequing CSV — each row lands in Unmatched, ready to reconcile."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-accent/10 border border-obsidian-accent/30 rounded-lg flex items-center justify-between hover:bg-obsidian-accent/15 hover:border-obsidian-accent/50 transition-colors text-left",
                onclick: move |_| on_open_reconciliation.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-accent", "Reconcile" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Pair Unmatched-touching transactions across sources — merge confirmed matches."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_balance_check.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Balance check" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Verify a cleared-transaction total against a statement closing balance."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
            button {
                class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
                onclick: move |_| on_open_journal_import.call(()),
                div {
                    div { class: "text-sm font-semibold text-obsidian-text", "Import journal" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Bring in an existing hledger journal. Preview accounts, drop or rename, then commit."
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
        }
    }
}

#[component]
fn CaptureTile(
    label: &'static str,
    icon_path: &'static str,
    enabled: bool,
    on_click: EventHandler<()>,
) -> Element {
    let base = "flex flex-col items-center justify-center gap-2 p-4 bg-obsidian-sidebar border border-white/10 rounded-xl min-h-[96px] transition-colors";
    let interactive = if enabled {
        "text-obsidian-text hover:border-obsidian-accent hover:text-obsidian-accent cursor-pointer"
    } else {
        "text-obsidian-text-muted opacity-50 cursor-not-allowed"
    };
    rsx! {
        button {
            class: "{base} {interactive}",
            disabled: !enabled,
            onclick: move |_| on_click.call(()),
            svg { class: "w-7 h-7", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: icon_path }
            }
            span { class: "text-sm font-medium", "{label}" }
        }
    }
}

// =============================================================================
// Document capture sub-view (Phase 3.1 photos + 3.2 PDFs)
//
// Flow: tile click → file picker → bytes ship to /documents/extract →
// structured ExtractedDraft renders below. PDFs surface a hint selector
// because `core::extraction::route_from_mime` deliberately returns None for
// application/pdf — receipt vs bank-statement vs paystub vs brokerage is
// not auto-derivable from the MIME alone.
// =============================================================================

const PDF_HINTS: &[(&str, &str)] = &[
    ("bank_statement", "Bank statement"),
    ("brokerage_statement", "Brokerage statement"),
    ("paystub", "Paystub"),
    ("receipt", "Receipt"),
];

#[component]
fn DocumentCapture(
    kind: DocumentKind,
    /// Bytes + metadata pre-loaded from an Android share-target SEND intent.
    /// When `Some`, the file picker is hidden in favor of a "Use shared file"
    /// confirm step (Phase 3.3); when `None`, the regular file-picker flow
    /// runs unchanged.
    #[props(default = None)]
    preloaded: Option<PendingShareCapture>,
    on_done: EventHandler<()>,
    on_extracted: EventHandler<ExtractedDraft>,
) -> Element {
    #[derive(Debug, Clone)]
    enum CaptureState {
        Idle,
        Working,
        Error {
            msg: String,
            retry_bytes: Option<(Vec<u8>, String)>,
        },
    }

    let (title, accept, prefer_camera, default_hint, show_hint_picker) = match kind {
        DocumentKind::Photo => ("Photo capture", "image/*", true, "receipt", false),
        DocumentKind::Pdf => (
            "PDF capture",
            "application/pdf",
            false,
            "bank_statement",
            true,
        ),
    };

    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);
    let mut hint = use_signal(|| default_hint.to_string());

    let on_file_picked = move |evt: Event<FormData>| {
        let files = evt.files();
        let Some(file) = files.into_iter().next() else {
            return;
        };
        let mime = file.content_type().unwrap_or_else(|| match kind {
            DocumentKind::Photo => "image/jpeg".to_string(),
            DocumentKind::Pdf => "application/pdf".to_string(),
        });
        let hint_value = hint.read().clone();

        state.set(CaptureState::Working);

        spawn(async move {
            let bytes = match file.read_bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    state.set(CaptureState::Error {
                        msg: format!("Couldn't read file: {e}"),
                        retry_bytes: None,
                    });
                    return;
                }
            };

            let retry_bytes = bytes.clone();
            let retry_mime = mime.clone();

            match bridge::invoke_extract_document(bytes, &mime, &hint_value).await {
                Ok(draft) => {
                    // Reset local state so a quick re-open shows the Idle
                    // prompt instead of a stale Working spinner.
                    state.set(CaptureState::Idle);
                    on_extracted.call(draft);
                }
                Err(e) => state.set(CaptureState::Error {
                    msg: format!("Couldn't extract: {e}"),
                    retry_bytes: Some((retry_bytes, retry_mime)),
                }),
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-6",

            // Back button + title
            div { class: "flex items-center gap-3 mb-2",
                button {
                    class: "text-obsidian-text-muted hover:text-obsidian-text text-sm flex items-center gap-1",
                    onclick: move |_| on_done.call(()),
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    span { "Back" }
                }
                h1 { class: "text-xl font-bold text-obsidian-accent", "{title}" }
            }

            // Hint selector — only renders for PDFs.
            if show_hint_picker {
                fieldset { class: "flex flex-col gap-2",
                    legend { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                        "What kind of PDF?"
                    }
                    div { class: "flex flex-wrap gap-2",
                        for (value, label) in PDF_HINTS.iter().copied() {
                            HintRadio {
                                value: value,
                                label: label,
                                checked: *hint.read() == value,
                                on_select: move |_| hint.set(value.to_string()),
                            }
                        }
                    }
                }
            }

            // Share-target preloaded panel — only renders when the user
            // arrived via an Android SEND intent (Phase 3.3). Surfaces the
            // shared file's metadata so the user can confirm before bytes
            // ship to Gemini; click fires the same extraction path the file
            // picker uses.
            if let Some(capture) = preloaded.clone() {
                div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
                    div { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                        "Shared file"
                    }
                    div { class: "flex items-center gap-2 text-sm text-obsidian-text",
                        svg { class: "w-4 h-4 text-obsidian-accent", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" }
                        }
                        span { class: "font-mono", "{capture.filename}" }
                        span { class: "text-obsidian-text-muted", " · {capture.size} bytes · {capture.mime}" }
                    }
                    button {
                        class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                        r#type: "button",
                        disabled: matches!(*state.read(), CaptureState::Working),
                        onclick: move |_| {
                            // Mirror on_file_picked's tail: bytes already in
                            // hand, skip the async read.
                            let bytes = capture.bytes.clone();
                            let mime = capture.mime.clone();
                            let hint_value = hint.read().clone();
                            let retry_bytes = bytes.clone();
                            let retry_mime = mime.clone();
                            state.set(CaptureState::Working);
                            spawn(async move {
                                match bridge::invoke_extract_document(bytes, &mime, &hint_value).await {
                                    Ok(draft) => {
                                        state.set(CaptureState::Idle);
                                        on_extracted.call(draft);
                                    }
                                    Err(e) => state.set(CaptureState::Error {
                                        msg: format!("Couldn't extract: {e}"),
                                        retry_bytes: Some((retry_bytes, retry_mime)),
                                    }),
                                }
                            });
                        },
                        "Use shared file"
                    }
                }
            } else {
                // File picker. `capture="environment"` only applies to the photo
                // flow — it tells mobile browsers to default to the rear camera.
                label { class: "block",
                    span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                        match kind {
                            DocumentKind::Photo => "Pick a photo or take one",
                            DocumentKind::Pdf => "Pick a PDF",
                        }
                    }
                    if prefer_camera {
                        input {
                            class: "block w-full text-sm text-obsidian-text file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:bg-obsidian-accent file:text-white file:font-medium hover:file:opacity-90 cursor-pointer",
                            r#type: "file",
                            accept: accept,
                            "capture": "environment",
                            onchange: on_file_picked,
                        }
                    } else {
                        input {
                            class: "block w-full text-sm text-obsidian-text file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:bg-obsidian-accent file:text-white file:font-medium hover:file:opacity-90 cursor-pointer",
                            r#type: "file",
                            accept: accept,
                            onchange: on_file_picked,
                        }
                    }
                }
            }

            // State-dependent body. The Error arm composes render_error with
            // a conditional Retry button that needs `state` in scope.
            div {
                {
                    match &*state.read() {
                        CaptureState::Idle => render_idle(kind),
                        CaptureState::Working => render_working(),
                        CaptureState::Error { msg, retry_bytes } => {
                            let retry = retry_bytes.clone();
                            let hint_value = hint.read().clone();
                            rsx! {
                                {render_error(msg)}
                                if let Some((bytes, mime)) = retry {
                                    div { class: "mt-3",
                                        button {
                                            class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90",
                                            onclick: move |_| {
                                                let bytes = bytes.clone();
                                                let mime = mime.clone();
                                                let hint_value = hint_value.clone();
                                                state.set(CaptureState::Working);
                                                spawn(async move {
                                                    let retry_bytes = bytes.clone();
                                                    let retry_mime = mime.clone();
                                                    match bridge::invoke_extract_document(bytes, &mime, &hint_value).await {
                                                        Ok(draft) => {
                                                            state.set(CaptureState::Idle);
                                                            on_extracted.call(draft);
                                                        }
                                                        Err(e) => state.set(CaptureState::Error {
                                                            msg: format!("Couldn't extract: {e}"),
                                                            retry_bytes: Some((retry_bytes, retry_mime)),
                                                        }),
                                                    }
                                                });
                                            },
                                            "Retry"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HintRadio(
    value: &'static str,
    label: &'static str,
    checked: bool,
    on_select: EventHandler<()>,
) -> Element {
    let base =
        "px-3 py-1.5 rounded-full text-xs font-medium border transition-colors cursor-pointer";
    let style = if checked {
        "bg-obsidian-accent text-white border-obsidian-accent"
    } else {
        "bg-transparent text-obsidian-text-muted border-white/10 hover:border-obsidian-accent hover:text-obsidian-text"
    };
    rsx! {
        button {
            class: "{base} {style}",
            r#type: "button",
            value: value,
            onclick: move |_| on_select.call(()),
            "{label}"
        }
    }
}

// =============================================================================
// Email body capture sub-view (Phase 3.4)
//
// A pasted email body — no file picker. User pastes the body text, clicks
// Extract, and the same /documents/extract endpoint handles it (hint=email_body,
// MIME=text/plain). Shares the state-machine pattern with DocumentCapture but
// the input shape is different enough that DRY-ing further would tangle the
// abstractions; the duplication is the bounded kind.
// =============================================================================

#[component]
fn EmailCapture(on_done: EventHandler<()>, on_extracted: EventHandler<ExtractedDraft>) -> Element {
    #[derive(Debug, Clone)]
    enum CaptureState {
        Idle,
        Working,
        Error {
            msg: String,
            retry_body: Option<String>,
        },
    }

    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);
    let mut body = use_signal(String::new);

    // FnMut because state.set requires &mut on the closure. Captured by move
    // into two onclicks (Extract + Retry); the closure is Copy because all
    // captures (Signal handles + EventHandler) are Copy.
    let mut kick_off = move |body_text: String| {
        if body_text.trim().is_empty() {
            return;
        }
        state.set(CaptureState::Working);
        spawn(async move {
            let bytes = body_text.clone().into_bytes();
            let retry_body = body_text;
            match bridge::invoke_extract_document(bytes, "text/plain", "email_body").await {
                Ok(draft) => {
                    state.set(CaptureState::Idle);
                    on_extracted.call(draft);
                }
                Err(e) => state.set(CaptureState::Error {
                    msg: format!("Couldn't extract: {e}"),
                    retry_body: Some(retry_body),
                }),
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-6",

            // Back button + title
            div { class: "flex items-center gap-3 mb-2",
                button {
                    class: "text-obsidian-text-muted hover:text-obsidian-text text-sm flex items-center gap-1",
                    onclick: move |_| on_done.call(()),
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    span { "Back" }
                }
                h1 { class: "text-xl font-bold text-obsidian-accent", "Email capture" }
            }

            // Paste area
            label { class: "block",
                span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Paste the email body"
                }
                textarea {
                    class: "block w-full min-h-[200px] p-3 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted text-sm font-mono outline-none focus:border-obsidian-accent transition-colors resize-y",
                    placeholder: "Paste a receipt confirmation email, a Wise notification, anything with a charge in it…",
                    value: "{body.read()}",
                    oninput: move |e| body.set(e.value().clone()),
                }
            }

            // Extract button
            div { class: "flex justify-end",
                button {
                    class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                    disabled: body.read().trim().is_empty() || matches!(*state.read(), CaptureState::Working),
                    onclick: move |_| {
                        let text = body.read().clone();
                        kick_off(text);
                    },
                    "Extract"
                }
            }

            // State-dependent body
            div {
                {
                    match &*state.read() {
                        CaptureState::Idle => rsx! {
                            p { class: "text-sm text-obsidian-text-muted",
                                "Paste a transaction-bearing email body above, then click Extract."
                            }
                        },
                        CaptureState::Working => render_working(),
                        CaptureState::Error { msg, retry_body } => {
                            let retry = retry_body.clone();
                            rsx! {
                                {render_error(msg)}
                                if let Some(text) = retry {
                                    div { class: "mt-3",
                                        button {
                                            class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90",
                                            onclick: move |_| {
                                                let text = text.clone();
                                                kick_off(text);
                                            },
                                            "Retry"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Transaction form — manual entry + confirm-draft (Phases 3.5 + 3.6)
//
// One form, two entry points:
//   - Manual tile         → initial = None,         empty form
//   - Extracted draft     → initial = Some(draft),  pre-populated
//
// Save → record_transaction Tauri command → TransactionRecorded event.
// =============================================================================

/// One editable posting row. Maps 1-1 to a backend `Posting` once the user
/// hits Save; `amount` stays a String here so the form can stage in-progress
/// text without bailing on every keystroke.
#[derive(Debug, Clone)]
struct PostingRow {
    account: String,
    commodity: String,
    amount: String,
}

impl PostingRow {
    fn empty(default_commodity: &str) -> Self {
        Self {
            account: String::new(),
            commodity: default_commodity.to_string(),
            amount: String::new(),
        }
    }
}

const DEFAULT_COMMODITY: &str = "CAD";

/// Continuity key for the in-flight capture (task 1.4). The UI only ever has one
/// `TransactionForm` open at a time, so a single fixed slot suffices — much like
/// `ContinuityKey::NewNote` for the notes draft.
fn active_capture_key() -> ContinuityKey {
    ContinuityKey::Capture("active".to_string())
}

#[component]
fn TransactionForm(initial: Option<ExtractedDraft>, on_done: EventHandler<()>) -> Element {
    let store = use_continuity();

    // Hydration precedence (1.4):
    //   1. a fresh extraction (`initial = Some`) wins over any stale stored
    //      draft — the user just captured something new;
    //   2. otherwise a stored draft means the user chose Resume (the Manual
    //      path clears the slot before navigating here, so reaching this arm
    //      with a draft is always a deliberate resume);
    //   3. otherwise a blank manual form.
    let (init_date, init_desc, init_rows, init_attachment) = if let Some(d) = initial {
        let rows: Vec<PostingRow> = if d.postings.is_empty() {
            vec![
                PostingRow::empty(DEFAULT_COMMODITY),
                PostingRow::empty(DEFAULT_COMMODITY),
            ]
        } else {
            d.postings
                .iter()
                .map(|p| PostingRow {
                    account: p.account_hint.clone().unwrap_or_default(),
                    commodity: p.commodity.clone(),
                    amount: p.amount.clone(),
                })
                .collect()
        };
        (
            d.date.unwrap_or_default(),
            d.description.unwrap_or_default(),
            rows,
            d.attachment,
        )
    } else if let Some(c) = store.peek_capture(&active_capture_key()) {
        // `peek`, not `get`: this one-time hydration must not subscribe the
        // render to the store, or the write-through mirror below would re-render
        // the form on every keystroke.
        let rows: Vec<PostingRow> = if c.postings.is_empty() {
            vec![
                PostingRow::empty(DEFAULT_COMMODITY),
                PostingRow::empty(DEFAULT_COMMODITY),
            ]
        } else {
            c.postings
                .into_iter()
                .map(|p| PostingRow {
                    account: p.account,
                    commodity: p.commodity,
                    amount: p.amount,
                })
                .collect()
        };
        (c.date, c.description, rows, c.attachment)
    } else {
        (
            String::new(),
            String::new(),
            vec![
                PostingRow::empty(DEFAULT_COMMODITY),
                PostingRow::empty(DEFAULT_COMMODITY),
            ],
            None,
        )
    };

    let mut date = use_signal(|| init_date);
    let mut description = use_signal(|| init_desc);
    let mut postings = use_signal(|| init_rows);
    let attachment: Signal<Option<AttachmentRef>> = use_signal(|| init_attachment);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Write-through mirror (1.4): stash the live form as a CaptureDraft so a tab
    // switch can't lose it. Empty drafts are removed rather than stored, so an
    // untouched blank form doesn't raise the Home "resume" affordance or linger.
    use_effect(move || {
        let draft = CaptureDraft {
            date: date.read().clone(),
            description: description.read().clone(),
            postings: postings
                .read()
                .iter()
                .map(|r| PostingDraft {
                    account: r.account.clone(),
                    commodity: r.commodity.clone(),
                    amount: r.amount.clone(),
                })
                .collect(),
            attachment: attachment.read().clone(),
        };
        let key = active_capture_key();
        if draft.is_empty() {
            store.remove_capture(&key);
        } else {
            store.put_capture(key, draft);
        }
    });

    let on_save = move |_| {
        if *saving.read() {
            return;
        }
        // Validate then build a TransactionFormDraft.
        let date_v = date.read().clone();
        let desc_v = description.read().clone();
        let rows = postings.read().clone();

        if date_v.trim().is_empty() {
            error.set(Some("Date is required.".into()));
            return;
        }
        if desc_v.trim().is_empty() {
            error.set(Some("Description is required.".into()));
            return;
        }
        let mut postings_out: Vec<PostingInput> = Vec::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            if r.account.trim().is_empty() && r.amount.trim().is_empty() {
                continue;
            }
            if r.account.trim().is_empty() {
                error.set(Some(format!("Row {} needs an account.", i + 1)));
                return;
            }
            if r.amount.trim().is_empty() {
                error.set(Some(format!("Row {} needs an amount.", i + 1)));
                return;
            }
            // Amount stays a String on the wire — the backend's
            // `rust_decimal::serde::str` adapter parses it. We do a quick
            // sanity-check here so a typo doesn't reach the server.
            if r.amount.parse::<f64>().is_err() {
                error.set(Some(format!(
                    "Row {}: '{}' is not a number.",
                    i + 1,
                    r.amount
                )));
                return;
            }
            postings_out.push(PostingInput {
                account: r.account.trim().to_string(),
                commodity: r.commodity.trim().to_string(),
                amount: r.amount.trim().to_string(),
                tags: Vec::new(),
            });
        }
        if postings_out.len() < 2 {
            error.set(Some(
                "At least two postings required (debit + credit).".into(),
            ));
            return;
        }

        error.set(None);
        saving.set(true);
        let submission = TransactionFormDraft {
            date: date_v,
            description: desc_v,
            postings: postings_out,
            attachment: attachment.read().clone(),
        };
        spawn(async move {
            match bridge::invoke_record_transaction(submission).await {
                Ok(()) => {
                    saving.set(false);
                    // Capture committed — drop the in-flight draft so it doesn't
                    // resurface as a "resume" on Home.
                    store.remove_capture(&active_capture_key());
                    on_done.call(());
                }
                Err(e) => {
                    saving.set(false);
                    error.set(Some(format!("Save failed: {e}")));
                }
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-6",

            // Back button + title
            div { class: "flex items-center gap-3 mb-2",
                button {
                    class: "text-obsidian-text-muted hover:text-obsidian-text text-sm flex items-center gap-1",
                    onclick: move |_| {
                        // Cancel abandons the in-flight capture.
                        store.remove_capture(&active_capture_key());
                        on_done.call(());
                    },
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    span { "Cancel" }
                }
                h1 { class: "text-xl font-bold text-obsidian-accent", "Transaction" }
            }

            // Date
            div {
                label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Date"
                }
                input {
                    class: "w-full px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text outline-none focus:border-obsidian-accent",
                    r#type: "date",
                    value: "{date.read()}",
                    oninput: move |e| date.set(e.value().clone()),
                }
            }

            // Description
            div {
                label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Description"
                }
                input {
                    class: "w-full px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text outline-none focus:border-obsidian-accent",
                    r#type: "text",
                    placeholder: "Loblaws — Groceries",
                    value: "{description.read()}",
                    oninput: move |e| description.set(e.value().clone()),
                }
            }

            // Postings list
            div { class: "flex flex-col gap-2",
                div { class: "flex items-center justify-between",
                    span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                        "Postings"
                    }
                    button {
                        class: "text-xs text-obsidian-accent hover:opacity-80",
                        r#type: "button",
                        onclick: move |_| {
                            let mut rows = postings.read().clone();
                            rows.push(PostingRow::empty(DEFAULT_COMMODITY));
                            postings.set(rows);
                        },
                        "+ Add posting"
                    }
                }

                {
                    let rows = postings.read().clone();
                    rsx! {
                        for (idx, row) in rows.into_iter().enumerate() {
                            div { key: "{idx}", class: "flex flex-wrap gap-2 items-center",
                                input {
                                    class: "flex-1 min-w-[200px] px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm outline-none focus:border-obsidian-accent",
                                    r#type: "text",
                                    placeholder: "Account (e.g. Expenses:Groceries)",
                                    value: "{row.account}",
                                    oninput: move |e| {
                                        let mut rows = postings.read().clone();
                                        if let Some(r) = rows.get_mut(idx) {
                                            r.account = e.value().clone();
                                        }
                                        postings.set(rows);
                                    },
                                }
                                input {
                                    class: "w-28 px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm font-mono outline-none focus:border-obsidian-accent",
                                    r#type: "text",
                                    placeholder: "0.00",
                                    value: "{row.amount}",
                                    oninput: move |e| {
                                        let mut rows = postings.read().clone();
                                        if let Some(r) = rows.get_mut(idx) {
                                            r.amount = e.value().clone();
                                        }
                                        postings.set(rows);
                                    },
                                }
                                input {
                                    class: "w-20 px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm font-mono outline-none focus:border-obsidian-accent uppercase",
                                    r#type: "text",
                                    placeholder: "CAD",
                                    value: "{row.commodity}",
                                    oninput: move |e| {
                                        let mut rows = postings.read().clone();
                                        if let Some(r) = rows.get_mut(idx) {
                                            r.commodity = e.value().clone();
                                        }
                                        postings.set(rows);
                                    },
                                }
                                button {
                                    class: "text-xs text-obsidian-text-muted hover:text-red-300 px-2 py-1",
                                    r#type: "button",
                                    disabled: postings.read().len() <= 2,
                                    onclick: move |_| {
                                        let mut rows = postings.read().clone();
                                        if rows.len() > 2 {
                                            rows.remove(idx);
                                            postings.set(rows);
                                        }
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }

            // Attachment indicator — surfaces the fact that bytes were
            // persisted server-side and mirrored to the on-device LRU cache.
            // Thumbnail rendering is Phase 4 (transaction detail) territory.
            if let Some(att) = attachment.read().clone() {
                div { class: "p-3 bg-obsidian-sidebar/60 border border-white/10 rounded-md text-xs text-obsidian-text-muted flex items-center gap-2",
                    svg { class: "w-4 h-4 text-obsidian-accent", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" }
                    }
                    span { "Attachment saved · " span { class: "font-mono", "{att.filename}" } " · {att.size} bytes" }
                }
            }

            // Error display
            if let Some(msg) = error.read().clone() {
                div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded-md text-sm text-red-300",
                    "{msg}"
                }
            }

            // Save
            div { class: "flex justify-end gap-2",
                button {
                    class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                    r#type: "button",
                    disabled: *saving.read(),
                    onclick: on_save,
                    if *saving.read() { "Saving…" } else { "Save transaction" }
                }
            }
        }
    }
}

// --- Render helpers ----------------------------------------------------------

fn render_idle(kind: DocumentKind) -> Element {
    let msg = match kind {
        DocumentKind::Photo => {
            "Pick a photo above to start. Mobile devices open the camera; desktop opens a file picker."
        }
        DocumentKind::Pdf => {
            "Pick a PDF above. Choose the document kind first so the extractor uses the right prompt."
        }
    };
    rsx! {
        p { class: "text-sm text-obsidian-text-muted", "{msg}" }
    }
}

fn render_working() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-4 bg-obsidian-sidebar/60 border border-white/5 rounded-lg",
            div { class: "w-4 h-4 border-2 border-obsidian-accent border-t-transparent rounded-full animate-spin" }
            span { class: "text-sm text-obsidian-text-muted", "Extracting transaction details…" }
        }
    }
}

fn render_error(message: &str) -> Element {
    rsx! {
        div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg space-y-2",
            p { class: "text-sm text-red-300", "Couldn't extract: {message}" }
            p { class: "text-xs text-obsidian-text-muted", "Pick another file above to retry." }
        }
    }
}

// =============================================================================
// Auto-import batch review (Phase 3.10.6)
//
// BatchListView: lists pending batches (one row per source/dedup) with quick
// metadata. Click → BatchReviewView.
//
// BatchReviewView: per-row accept toggle; if any row carries a commodity in
// MANUAL_FX_CURRENCIES (i.e., NGN today), prompts for the manual base→quote
// rate before Commit. Commit fans out to TransactionRecorded × N + optional
// ExchangeRateRecorded + AutoImportBatchCommitted via `commit_batch`.
// =============================================================================

/// Currencies that need a user-supplied FX rate because Frankfurter doesn't
/// cover them. Mirror of `core::fx::MANUAL_FX_CURRENCIES`; kept in sync via
/// the cross-crate type round-trip test in `core/src/fx.rs`. UI-side dup is
/// pragmatic — making this WASM-shared would pull `core` into the frontend.
const MANUAL_FX_CURRENCIES_UI: &[&str] = &["NGN"];

fn batch_needs_manual_fx(batch: &PendingBatchView) -> Option<String> {
    for draft in &batch.draft_postings {
        for posting in &draft.postings {
            for commodity in MANUAL_FX_CURRENCIES_UI {
                if posting.commodity.eq_ignore_ascii_case(commodity) {
                    return Some((*commodity).to_string());
                }
            }
        }
    }
    None
}

#[component]
fn BatchListView(
    on_back: EventHandler<()>,
    on_open_batch: EventHandler<String>,
) -> Element {
    let mut batches: Signal<Option<Result<Vec<PendingBatchView>, String>>> =
        use_signal(|| None);

    use_effect(move || {
        spawn(async move {
            batches.set(Some(bridge::invoke_list_pending_batches().await));
        });
    });

    rsx! {
        div { class: "flex items-center justify-between mb-6",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Auto-import review"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        match batches.read().clone() {
            None => rsx! {
                div { class: "text-obsidian-text-muted text-sm", "Loading pending batches…" }
            },
            Some(Err(msg)) => rsx! {
                div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                    "Failed to load pending batches: {msg}"
                }
            },
            Some(Ok(rows)) if rows.is_empty() => rsx! {
                div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                    "No pending auto-import batches. Captured transactions and confirmed batches show up in Recent (Phase 4)."
                }
            },
            Some(Ok(rows)) => rsx! {
                div { class: "space-y-2",
                    for batch in rows {
                        BatchListRow {
                            key: "{batch.batch_id}",
                            batch: batch.clone(),
                            on_open: {
                                let id = batch.batch_id.clone();
                                let handler = on_open_batch;
                                move |_| handler.call(id.clone())
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BatchListRow(batch: PendingBatchView, on_open: EventHandler<()>) -> Element {
    let row_count = batch.draft_postings.len();
    let fx_hint = batch_needs_manual_fx(&batch);
    let source_label = pretty_source(&batch.source);
    let fetched_short = batch
        .fetched_at
        .split('T')
        .next()
        .unwrap_or(&batch.fetched_at)
        .to_string();

    rsx! {
        button {
            class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
            onclick: move |_| on_open.call(()),
            div { class: "flex-1 min-w-0",
                div { class: "flex items-baseline gap-2 mb-1",
                    span { class: "text-sm font-semibold text-obsidian-text", "{source_label}" }
                    span { class: "text-xs text-obsidian-text-muted", "· {fetched_short}" }
                    if let Some(c) = fx_hint {
                        span { class: "text-xs px-2 py-0.5 bg-amber-500/15 text-amber-300 rounded-full",
                            "needs {c} rate"
                        }
                    }
                }
                div { class: "text-xs text-obsidian-text-muted truncate",
                    if row_count == 1 { "1 transaction" } else { "{row_count} transactions" }
                }
            }
            svg { class: "w-5 h-5 text-obsidian-text-muted shrink-0",
                fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                    d: "M9 5l7 7-7 7"
                }
            }
        }
    }
}

fn pretty_source(source: &str) -> &str {
    match source {
        "wise" => "Wise",
        "wealthsimple" | "wealthsimple-snaptrade" => "WealthSimple",
        "sc_ngn" | "imap-standardchartered-ngn" => "Standard Chartered (NGN)",
        "imap_receipts" | "imap-receipts" | "receipts" => "Email receipts",
        other => other,
    }
}

#[component]
fn BatchReviewView(batch_id: String, on_done: EventHandler<()>) -> Element {
    let batch_id_for_resource = batch_id.clone();
    let mut batch: Signal<Option<Result<PendingBatchView, String>>> = use_signal(|| None);
    let mut accepted: Signal<Vec<bool>> = use_signal(Vec::new);
    let mut fx_rate_input: Signal<String> = use_signal(String::new);
    let mut busy: Signal<bool> = use_signal(|| false);
    let mut feedback: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        let id = batch_id_for_resource.clone();
        spawn(async move {
            match bridge::invoke_list_pending_batches().await {
                Ok(rows) => match rows.into_iter().find(|b| b.batch_id == id) {
                    Some(found) => {
                        accepted.set(vec![true; found.draft_postings.len()]);
                        batch.set(Some(Ok(found)));
                    }
                    None => batch.set(Some(Err(format!(
                        "Batch {id} no longer pending — it may have been resolved on another device."
                    )))),
                },
                Err(e) => batch.set(Some(Err(e))),
            }
        });
    });

    let current = batch.read().clone();
    match current {
        None => rsx! {
            div { class: "text-obsidian-text-muted text-sm", "Loading batch…" }
        },
        Some(Err(msg)) => rsx! {
            div { class: "space-y-4",
                div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                    "{msg}"
                }
                button {
                    class: "text-sm text-obsidian-text-muted hover:text-obsidian-text underline",
                    onclick: move |_| on_done.call(()),
                    "← Back to list"
                }
            }
        },
        Some(Ok(b)) => {
            let manual_fx_commodity = batch_needs_manual_fx(&b);
            let source_label = pretty_source(&b.source).to_string();
            let row_count = b.draft_postings.len();
            let accepted_count = accepted.read().iter().filter(|x| **x).count();
            let metadata_pretty = b.source_metadata.as_ref().and_then(|v| {
                serde_json::to_string_pretty(v).ok()
            });

            rsx! {
                div { class: "flex items-center justify-between mb-4",
                    div {
                        h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                            "{source_label}"
                        }
                        p { class: "text-xs text-obsidian-text-muted mt-1",
                            "Fetched {b.fetched_at} · {row_count} draft transactions"
                        }
                    }
                    button {
                        class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                        onclick: move |_| on_done.call(()),
                        "← List"
                    }
                }

                if let Some(meta_str) = metadata_pretty {
                    details { class: "mb-4 text-xs text-obsidian-text-muted",
                        summary { class: "cursor-pointer hover:text-obsidian-text", "Source metadata" }
                        pre { class: "mt-2 p-3 bg-obsidian-sidebar/60 rounded border border-white/5 overflow-x-auto",
                            "{meta_str}"
                        }
                    }
                }

                div { class: "space-y-2 mb-6",
                    for (idx, draft) in b.draft_postings.iter().enumerate() {
                        DraftRow {
                            key: "{draft.external_id}",
                            idx: idx,
                            draft: draft.clone(),
                            accepted: accepted.read().get(idx).copied().unwrap_or(true),
                            on_toggle: move |_| {
                                let mut current_accepted = accepted.write();
                                if let Some(slot) = current_accepted.get_mut(idx) {
                                    *slot = !*slot;
                                }
                            },
                        }
                    }
                }

                if let Some(commodity) = manual_fx_commodity.clone() {
                    div { class: "mb-6 p-4 bg-amber-500/10 border border-amber-500/30 rounded-lg",
                        label { class: "block text-sm font-semibold text-amber-200 mb-2",
                            "Manual FX rate (1 {commodity} = ? CAD)"
                        }
                        p { class: "text-xs text-obsidian-text-muted mb-3",
                            "Required because {commodity} is outside the daily-rate provider's coverage. The rate is recorded as an hledger P directive at the batch's effective date."
                        }
                        input {
                            r#type: "text",
                            inputmode: "decimal",
                            placeholder: "e.g., 0.00088",
                            class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-obsidian-text",
                            value: "{fx_rate_input.read()}",
                            oninput: move |evt| fx_rate_input.set(evt.value()),
                        }
                    }
                }

                div { class: "flex gap-3 items-center",
                    button {
                        class: "flex-1 px-4 py-3 bg-obsidian-accent text-black font-semibold rounded-lg hover:bg-obsidian-accent/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *busy.read() || accepted_count == 0
                            || (manual_fx_commodity.is_some() && fx_rate_input.read().trim().is_empty()),
                        onclick: {
                            let batch_id = b.batch_id.clone();
                            let manual_fx_commodity = manual_fx_commodity.clone();
                            move |_| {
                                let batch_id = batch_id.clone();
                                let manual_fx_commodity = manual_fx_commodity.clone();
                                let accepted_indices: Vec<usize> = accepted
                                    .read()
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, on)| if *on { Some(i) } else { None })
                                    .collect();
                                let rate = fx_rate_input.read().trim().to_string();
                                spawn(async move {
                                    busy.set(true);
                                    feedback.set(None);
                                    let (fx_rate, fx_commodity) = match manual_fx_commodity {
                                        Some(c) if !rate.is_empty() => (Some(rate), Some(c)),
                                        _ => (None, None),
                                    };
                                    let res = bridge::invoke_commit_batch(
                                        &batch_id,
                                        accepted_indices,
                                        fx_rate,
                                        fx_commodity,
                                    )
                                    .await;
                                    busy.set(false);
                                    match res {
                                        Ok(_) => on_done.call(()),
                                        Err(e) => feedback.set(Some(format!("Commit failed: {e}"))),
                                    }
                                });
                            }
                        },
                        if *busy.read() {
                            "Committing…"
                        } else if accepted_count == row_count {
                            "Commit all {accepted_count}"
                        } else {
                            "Commit {accepted_count} of {row_count}"
                        }
                    }
                    button {
                        class: "px-4 py-3 bg-obsidian-sidebar border border-white/10 text-obsidian-text-muted rounded-lg hover:text-obsidian-text hover:border-red-500/40 hover:text-red-300 transition-colors disabled:opacity-50",
                        disabled: *busy.read(),
                        onclick: {
                            let batch_id = b.batch_id.clone();
                            move |_| {
                                let batch_id = batch_id.clone();
                                spawn(async move {
                                    busy.set(true);
                                    feedback.set(None);
                                    let res = bridge::invoke_dismiss_batch(&batch_id, None).await;
                                    busy.set(false);
                                    match res {
                                        Ok(()) => on_done.call(()),
                                        Err(e) => feedback.set(Some(format!("Dismiss failed: {e}"))),
                                    }
                                });
                            }
                        },
                        "Dismiss"
                    }
                }

                if let Some(msg) = feedback.read().clone() {
                    div { class: "mt-4 p-3 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                        "{msg}"
                    }
                }
            }
        }
    }
}

#[component]
fn DraftRow(
    idx: usize,
    draft: DraftTransactionView,
    accepted: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let border = if accepted {
        "border-obsidian-accent/40"
    } else {
        "border-white/5 opacity-60"
    };
    rsx! {
        div { class: "p-3 bg-obsidian-sidebar/60 border {border} rounded-lg",
            div { class: "flex items-start gap-3",
                input {
                    r#type: "checkbox",
                    class: "mt-1",
                    checked: accepted,
                    onchange: move |_| on_toggle.call(()),
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-baseline gap-2 mb-1",
                        span { class: "text-xs text-obsidian-text-muted font-mono", "#{idx + 1}" }
                        span { class: "text-sm font-medium text-obsidian-text", "{draft.date}" }
                    }
                    div { class: "text-sm text-obsidian-text truncate mb-2", "{draft.description}" }
                    div { class: "space-y-1",
                        for posting in &draft.postings {
                            div { class: "flex justify-between gap-3 text-xs",
                                span { class: "text-obsidian-text-muted font-mono truncate", "{posting.account}" }
                                span { class: "text-obsidian-text font-mono shrink-0",
                                    "{posting.amount} {posting.commodity}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Transaction list (Phase 4.1)
//
// Browse-first slice: paginated `list_transactions` reads, no filters yet,
// rows are non-clickable (4.2 lands the detail view). Filter chips +
// click-through both layer on top of this shell without restructuring.
// =============================================================================

const TXN_PAGE_SIZE: u32 = 50;

/// Minimal posting view extracted from a `TransactionView.postings` JSON
/// array. The full backend `Posting` carries `fx_rate` + `tags` too; rows
/// only need the at-a-glance fields. Detail view (4.2) will re-decode the
/// full shape.
#[derive(Debug, Clone)]
struct PostingRowView {
    account: String,
    amount: String,
    commodity: String,
}

/// Inline category-edit chip. Click → input replaces chip. Save on Enter or
/// button; Cancel on Escape. Empty trimmed value clears the category. Used
/// inside both the list row (where parent clicks navigate to detail — chip
/// click events must `stop_propagation`) and the detail body.
#[component]
fn EditableCategoryChip(
    current: Option<String>,
    on_save: EventHandler<String>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| current.clone().unwrap_or_default());

    let current_display = current.clone();

    rsx! {
        if *editing.read() {
            div {
                class: "inline-flex items-center gap-1",
                onclick: move |e| e.stop_propagation(),
                input {
                    r#type: "text",
                    class: "px-2 py-0.5 text-[10px] bg-obsidian-bg border border-obsidian-accent rounded-full text-obsidian-text outline-none w-32",
                    placeholder: "Category",
                    value: "{draft.read()}",
                    autofocus: true,
                    oninput: move |e| draft.set(e.value()),
                    onkeydown: move |e| {
                        let key = e.key().to_string();
                        if key == "Enter" {
                            let value = draft.read().trim().to_string();
                            on_save.call(value);
                            editing.set(false);
                        } else if key == "Escape" {
                            draft.set(current_display.clone().unwrap_or_default());
                            editing.set(false);
                        }
                    },
                }
                button {
                    r#type: "button",
                    class: "text-[10px] text-obsidian-accent hover:underline",
                    onclick: move |e| {
                        e.stop_propagation();
                        let value = draft.read().trim().to_string();
                        on_save.call(value);
                        editing.set(false);
                    },
                    "Save"
                }
                button {
                    r#type: "button",
                    class: "text-[10px] text-obsidian-text-muted hover:text-obsidian-text",
                    onclick: move |e| {
                        e.stop_propagation();
                        draft.set(current.clone().unwrap_or_default());
                        editing.set(false);
                    },
                    "Cancel"
                }
            }
        } else {
            button {
                r#type: "button",
                class: if current.is_some() {
                    "text-[10px] px-2 py-0.5 bg-obsidian-accent/15 text-obsidian-accent rounded-full font-medium hover:bg-obsidian-accent/25"
                } else {
                    "text-[10px] px-2 py-0.5 bg-white/5 text-obsidian-text-muted rounded-full font-medium hover:bg-white/10 border border-dashed border-white/10"
                },
                onclick: move |e| {
                    e.stop_propagation();
                    editing.set(true);
                },
                {
                    match current.clone() {
                        Some(c) if !c.is_empty() => rsx!{ "{c}" },
                        _ => rsx!{ "+ category" },
                    }
                }
            }
        }
    }
}

/// Inline tag-list editor. Existing tags render with × removers; trailing
/// "+ tag" enters an input. Like the category chip, all interactions
/// `stop_propagation` so the parent row's click handler doesn't fire.
#[component]
fn EditableTagList(
    current: Vec<String>,
    on_save: EventHandler<Vec<String>>,
) -> Element {
    let mut adding = use_signal(|| false);
    let mut draft = use_signal(String::new);

    rsx! {
        div {
            class: "inline-flex flex-wrap items-center gap-1",
            onclick: move |e| e.stop_propagation(),
            for (idx, tag) in current.iter().cloned().enumerate() {
                span { class: "text-[10px] inline-flex items-center gap-1 px-2 py-0.5 bg-white/5 text-obsidian-text-muted rounded-full font-mono",
                    "#{tag}"
                    button {
                        r#type: "button",
                        class: "text-obsidian-text-muted hover:text-red-300 leading-none",
                        onclick: {
                            let tags = current.clone();
                            move |e: dioxus::prelude::Event<dioxus::prelude::MouseData>| {
                                e.stop_propagation();
                                let mut next = tags.clone();
                                if idx < next.len() {
                                    next.remove(idx);
                                    on_save.call(next);
                                }
                            }
                        },
                        "×"
                    }
                }
            }
            if *adding.read() {
                input {
                    r#type: "text",
                    class: "px-2 py-0.5 text-[10px] bg-obsidian-bg border border-obsidian-accent rounded-full text-obsidian-text outline-none w-24",
                    placeholder: "tag",
                    value: "{draft.read()}",
                    autofocus: true,
                    oninput: move |e| draft.set(e.value()),
                    onkeydown: {
                        let tags = current.clone();
                        move |e: dioxus::prelude::Event<dioxus::prelude::KeyboardData>| {
                            let key = e.key().to_string();
                            if key == "Enter" {
                                let value = draft.read().trim().to_string();
                                if !value.is_empty() && !tags.iter().any(|t| t == &value) {
                                    let mut next = tags.clone();
                                    next.push(value);
                                    on_save.call(next);
                                }
                                draft.set(String::new());
                                adding.set(false);
                            } else if key == "Escape" {
                                draft.set(String::new());
                                adding.set(false);
                            }
                        }
                    },
                    onblur: {
                        let tags = current.clone();
                        move |_| {
                            let value = draft.read().trim().to_string();
                            if !value.is_empty() && !tags.iter().any(|t| t == &value) {
                                let mut next = tags.clone();
                                next.push(value);
                                on_save.call(next);
                            }
                            draft.set(String::new());
                            adding.set(false);
                        }
                    },
                }
            } else {
                button {
                    r#type: "button",
                    class: "text-[10px] px-2 py-0.5 bg-transparent text-obsidian-text-muted rounded-full font-mono border border-dashed border-white/10 hover:border-obsidian-accent hover:text-obsidian-accent",
                    onclick: move |e| {
                        e.stop_propagation();
                        adding.set(true);
                    },
                    "+ tag"
                }
            }
        }
    }
}

/// Decode `TransactionView.postings` (serde_json::Value, FLEXIBLE on the
/// backend) into a Vec of display rows. Postings missing required fields
/// are silently dropped — `record_transaction` validates upstream so this
/// is defensive only.
fn posting_views(value: &serde_json::Value) -> Vec<PostingRowView> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(PostingRowView {
                        account: p.get("account")?.as_str()?.to_string(),
                        amount: p.get("amount")?.as_str()?.to_string(),
                        commodity: p.get("commodity")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[component]
fn TransactionListView(
    on_back: EventHandler<()>,
    on_open_txn: EventHandler<String>,
    /// Optional initial filter seed — used when entering the screen from a
    /// dashboard widget click-through (e.g. Unmatched balance → list
    /// filtered to `account: "Unmatched"`). None = blank filter.
    #[props(default = None)]
    initial_filter: Option<TxnFilter>,
) -> Element {
    let store = use_continuity();
    let list_key = ContinuityKey::TxnList("main".to_string());

    // Continuity (1.5): restore the list across a detail round-trip. Precedence
    // mirrors the capture flow — a dashboard click-through (`initial_filter =
    // Some`) is a deliberate new query and overrides any restored list;
    // otherwise a stored list state means we're returning from detail and
    // should rehydrate rows + offset + filter instead of re-fetching page 0.
    let restored = if initial_filter.is_some() {
        None
    } else {
        store.peek_list(&list_key)
    };
    let seed = initial_filter.clone().unwrap_or_default();
    let (init_txns, init_offset, init_has_more, init_filter, init_loading) = match &restored {
        Some(ls) => (
            ls.transactions.clone(),
            ls.offset,
            ls.has_more,
            ls.filter.clone(),
            false,
        ),
        None => (Vec::new(), 0, false, seed.clone(), true),
    };

    let mut transactions: Signal<Vec<TransactionView>> = use_signal(|| init_txns);
    let mut loading: Signal<bool> = use_signal(|| init_loading);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut has_more: Signal<bool> = use_signal(|| init_has_more);
    let mut offset: Signal<u32> = use_signal(|| init_offset);
    // `active_filter` is what the current page reflects. `draft_filter` is
    // what the FilterBar inputs hold; only Apply copies draft → active.
    // Keeping them separate means typing in a field doesn't fire queries
    // and accidental edits don't invalidate the current page until Apply.
    let mut active_filter: Signal<TxnFilter> = use_signal(|| init_filter.clone());
    let draft_filter: Signal<TxnFilter> = use_signal(|| init_filter.clone());
    // One-shot: true only when we hydrated from the store, so the load effect
    // below skips its first fetch (the rows are already populated).
    let mut restored_pending = use_signal(|| restored.is_some());

    // Load (or re-load) the first page whenever active_filter changes — except
    // on the first run after a store-restore, where re-fetching would discard
    // the rehydrated rows and scroll position.
    use_effect(move || {
        let filter = active_filter.read().clone();
        // peek (not read) so flipping the flag doesn't re-trigger this effect.
        if *restored_pending.peek() {
            restored_pending.set(false);
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match bridge::invoke_list_transactions(filter, TXN_PAGE_SIZE, 0).await {
                Ok(rows) => {
                    has_more.set(rows.len() as u32 == TXN_PAGE_SIZE);
                    offset.set(rows.len() as u32);
                    transactions.set(rows);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    // Write-through mirror (1.5): keep the stored list current so the detail
    // round-trip restores the latest rows / offset / filter.
    {
        let key = list_key.clone();
        use_effect(move || {
            let state = ListState {
                transactions: transactions.read().clone(),
                offset: *offset.read(),
                has_more: *has_more.read(),
                filter: active_filter.read().clone(),
            };
            store.put_list(key.clone(), state);
        });
    }

    let load_more = move |_| {
        if *loading.read() {
            return;
        }
        let current_offset = *offset.read();
        let filter = active_filter.read().clone();
        spawn(async move {
            loading.set(true);
            match bridge::invoke_list_transactions(filter, TXN_PAGE_SIZE, current_offset).await {
                Ok(rows) => {
                    has_more.set(rows.len() as u32 == TXN_PAGE_SIZE);
                    let mut all = transactions.read().clone();
                    all.extend(rows);
                    offset.set(all.len() as u32);
                    transactions.set(all);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    let rows = transactions.read().clone();
    let is_loading = *loading.read();
    let err_msg = error.read().clone();
    let show_empty = rows.is_empty() && !is_loading && err_msg.is_none();
    let filter_active = !active_filter.read().is_empty();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Transactions"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        FilterBar {
            draft: draft_filter,
            on_apply: move |applied: TxnFilter| {
                active_filter.set(applied);
            },
            on_clear: move |_| {
                draft_filter.clone().set(TxnFilter::default());
                active_filter.set(TxnFilter::default());
            },
            filter_active: filter_active,
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "Failed to load transactions: {msg}"
            }
        }

        if show_empty {
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                if filter_active {
                    "No transactions match these filters."
                } else {
                    "No transactions yet. Capture one from the Finances home."
                }
            }
        } else {
            div { class: "space-y-2",
                for txn in rows {
                    TransactionListRow {
                        key: "{txn.id}",
                        txn: txn.clone(),
                        on_click: {
                            let id = txn.id.clone();
                            let handler = on_open_txn;
                            move |_| handler.call(id.clone())
                        },
                    }
                }
            }

            if is_loading {
                div { class: "mt-4 p-4 text-center text-obsidian-text-muted text-sm",
                    "Loading…"
                }
            } else if *has_more.read() {
                div { class: "mt-4 flex justify-center",
                    button {
                        class: "px-4 py-2 bg-obsidian-sidebar border border-white/10 text-obsidian-text-muted text-sm rounded-md hover:border-obsidian-accent/40 hover:text-obsidian-text transition-colors",
                        onclick: load_more,
                        "Load more"
                    }
                }
            }
        }
    }
}

/// Which field a query-builder row filters on. Mirrors the `core::query`
/// grammar's field keys; the builder only ever emits valid DSL.
#[derive(Clone, Copy, PartialEq)]
enum QField {
    Account,
    Tag,
    Date,
    Amount,
    Commodity,
    Description,
}

impl QField {
    fn as_key(self) -> &'static str {
        match self {
            QField::Account => "account",
            QField::Tag => "tag",
            QField::Date => "date",
            QField::Amount => "amount",
            QField::Commodity => "commodity",
            QField::Description => "description",
        }
    }
    fn from_key(s: &str) -> Self {
        match s {
            "tag" => QField::Tag,
            "date" => QField::Date,
            "amount" => QField::Amount,
            "commodity" => QField::Commodity,
            "description" => QField::Description,
            _ => QField::Account,
        }
    }
    fn placeholder(self) -> &'static str {
        match self {
            QField::Account => "Expenses:Food",
            QField::Tag => "business  or  type:business",
            QField::Date => "YYYY-MM-DD or YYYY-MM",
            QField::Amount => "0.00",
            QField::Commodity => "CAD",
            QField::Description => "text contains…",
        }
    }
}

/// One builder row. All state lives in the parent's `rows` signal, so the inputs
/// are controlled and index-keying the list is safe (no per-row local state).
#[derive(Clone, PartialEq)]
struct QueryRow {
    field: QField,
    /// Primary value: account path / tag / date-from / amount / commodity / desc.
    value: String,
    /// Secondary value: date-to (Date field only).
    value2: String,
    /// Account field only — checked emits subtree match, unchecked appends `$`.
    include_subtree: bool,
    /// Amount field only — the comparison operator the DSL emits.
    amount_op: String,
}

impl QueryRow {
    fn new() -> Self {
        QueryRow {
            field: QField::Account,
            value: String::new(),
            value2: String::new(),
            include_subtree: true,
            amount_op: ">=".into(),
        }
    }
}

/// Assemble the canonical query DSL from builder rows. Empty rows are skipped;
/// this mirrors `core::query::parser` so whatever it emits round-trips through
/// the host engine.
fn build_query_dsl(rows: &[QueryRow], any: bool) -> String {
    let mut terms: Vec<String> = Vec::new();
    for r in rows {
        let v = r.value.trim();
        match r.field {
            QField::Account => {
                if v.is_empty() {
                    continue;
                }
                let anchor = if r.include_subtree { "" } else { "$" };
                terms.push(format!("account:{v}{anchor}"));
            }
            QField::Tag => {
                if !v.is_empty() {
                    terms.push(format!("tag:{v}"));
                }
            }
            QField::Date => {
                let to = r.value2.trim();
                if v.is_empty() && to.is_empty() {
                    continue;
                }
                terms.push(format!("date:{v}..{to}"));
            }
            QField::Amount => {
                if !v.is_empty() {
                    terms.push(format!("amount:{}{}", r.amount_op, v));
                }
            }
            QField::Commodity => {
                if !v.is_empty() {
                    terms.push(format!("commodity:{v}"));
                }
            }
            QField::Description => {
                if v.is_empty() {
                    continue;
                }
                if v.chars().any(char::is_whitespace) {
                    terms.push(format!("desc:\"{v}\""));
                } else {
                    terms.push(format!("desc:{v}"));
                }
            }
        }
    }
    let sep = if any { " OR " } else { " " };
    terms.join(sep)
}

/// R2 ad-hoc query builder (Phase 7.1). Compose field predicates, watch the
/// generated DSL update live, run it against the host engine (Phase 7.2), and
/// browse matching transactions with the same row component as the browse list.
#[component]
fn QueryBuilderView(on_back: EventHandler<()>, on_open_txn: EventHandler<String>) -> Element {
    let mut rows: Signal<Vec<QueryRow>> = use_signal(|| vec![QueryRow::new()]);
    let mut any: Signal<bool> = use_signal(|| false);
    let mut results: Signal<Vec<TransactionView>> = use_signal(Vec::new);
    let mut loading: Signal<bool> = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut has_run: Signal<bool> = use_signal(|| false);

    let input_class = "px-2 py-1.5 bg-obsidian-bg border border-white/10 rounded-md text-obsidian-text text-xs outline-none focus:border-obsidian-accent placeholder-obsidian-text-muted";
    let select_class = "px-2 py-1.5 bg-obsidian-bg border border-white/10 rounded-md text-obsidian-text text-xs outline-none focus:border-obsidian-accent";

    let dsl = build_query_dsl(&rows.read(), *any.read());
    let dsl_empty = dsl.trim().is_empty();

    let run = {
        let dsl = dsl.clone();
        move |_| {
            let dsl = dsl.clone();
            spawn(async move {
                loading.set(true);
                error.set(None);
                has_run.set(true);
                match bridge::invoke_run_transaction_query(&dsl, TXN_PAGE_SIZE, 0).await {
                    Ok(r) => results.set(r),
                    Err(e) => {
                        error.set(Some(e));
                        results.set(Vec::new());
                    }
                }
                loading.set(false);
            });
        }
    };

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent", "Query transactions" }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        div { class: "mb-4 flex items-center gap-2 text-sm",
            span { class: "text-obsidian-text-muted", "Match" }
            button {
                class: if !*any.read() {
                    "px-3 py-1 rounded-md bg-obsidian-accent text-black font-semibold"
                } else {
                    "px-3 py-1 rounded-md bg-obsidian-sidebar border border-white/10 text-obsidian-text-muted hover:text-obsidian-text"
                },
                onclick: move |_| any.set(false),
                "ALL"
            }
            button {
                class: if *any.read() {
                    "px-3 py-1 rounded-md bg-obsidian-accent text-black font-semibold"
                } else {
                    "px-3 py-1 rounded-md bg-obsidian-sidebar border border-white/10 text-obsidian-text-muted hover:text-obsidian-text"
                },
                onclick: move |_| any.set(true),
                "ANY"
            }
            span { class: "text-obsidian-text-muted text-xs", "of the filters below" }
        }

        div { class: "space-y-2",
            for (i, row) in rows.read().clone().into_iter().enumerate() {
                div {
                    key: "{i}",
                    class: "flex flex-wrap items-center gap-2 p-2 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
                    select {
                        class: "{select_class}",
                        value: "{row.field.as_key()}",
                        onchange: move |e| {
                            rows.write()[i].field = QField::from_key(&e.value());
                        },
                        option { value: "account", "Account" }
                        option { value: "tag", "Tag" }
                        option { value: "date", "Date" }
                        option { value: "amount", "Amount" }
                        option { value: "commodity", "Commodity" }
                        option { value: "description", "Description" }
                    }

                    match row.field {
                        QField::Account => rsx! {
                            input {
                                r#type: "text",
                                class: "{input_class} flex-1 min-w-[140px]",
                                placeholder: "{QField::Account.placeholder()}",
                                value: "{row.value}",
                                oninput: move |e| rows.write()[i].value = e.value(),
                            }
                            label { class: "flex items-center gap-1 text-xs text-obsidian-text-muted whitespace-nowrap",
                                input {
                                    r#type: "checkbox",
                                    checked: row.include_subtree,
                                    onchange: move |e| rows.write()[i].include_subtree = e.value() == "true",
                                }
                                "Include sub-accounts"
                            }
                        },
                        QField::Amount => rsx! {
                            select {
                                class: "{select_class}",
                                value: "{row.amount_op}",
                                onchange: move |e| rows.write()[i].amount_op = e.value(),
                                option { value: ">=", "≥" }
                                option { value: ">", ">" }
                                option { value: "<=", "≤" }
                                option { value: "<", "<" }
                                option { value: "=", "=" }
                            }
                            input {
                                r#type: "text",
                                class: "{input_class} flex-1 min-w-[100px]",
                                placeholder: "{QField::Amount.placeholder()}",
                                value: "{row.value}",
                                oninput: move |e| rows.write()[i].value = e.value(),
                            }
                        },
                        QField::Date => rsx! {
                            input {
                                r#type: "text",
                                class: "{input_class} flex-1 min-w-[120px]",
                                placeholder: "from — {QField::Date.placeholder()}",
                                value: "{row.value}",
                                oninput: move |e| rows.write()[i].value = e.value(),
                            }
                            span { class: "text-obsidian-text-muted text-xs", "to" }
                            input {
                                r#type: "text",
                                class: "{input_class} flex-1 min-w-[120px]",
                                placeholder: "(optional)",
                                value: "{row.value2}",
                                oninput: move |e| rows.write()[i].value2 = e.value(),
                            }
                        },
                        _ => rsx! {
                            input {
                                r#type: "text",
                                class: "{input_class} flex-1 min-w-[140px]",
                                placeholder: "{row.field.placeholder()}",
                                value: "{row.value}",
                                oninput: move |e| rows.write()[i].value = e.value(),
                            }
                        },
                    }

                    button {
                        class: "ml-auto text-obsidian-text-muted hover:text-red-400 text-sm px-2",
                        title: "Remove filter",
                        onclick: move |_| {
                            let mut w = rows.write();
                            if w.len() > 1 {
                                w.remove(i);
                            }
                        },
                        "✕"
                    }
                }
            }
        }

        button {
            class: "mt-3 text-sm text-obsidian-accent hover:underline",
            onclick: move |_| rows.write().push(QueryRow::new()),
            "+ Add filter"
        }

        div { class: "mt-4",
            div { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest mb-1", "Generated query" }
            div {
                class: "font-mono text-xs bg-obsidian-bg border border-white/10 rounded-md px-3 py-2 text-obsidian-text break-all min-h-[2.25rem]",
                if dsl_empty {
                    span { class: "text-obsidian-text-muted", "(add a filter above)" }
                } else {
                    "{dsl}"
                }
            }
        }

        div { class: "mt-4",
            button {
                class: "px-4 py-2 bg-obsidian-accent text-black font-semibold text-sm rounded-md hover:bg-obsidian-accent/90 disabled:opacity-40 disabled:cursor-not-allowed",
                disabled: dsl_empty,
                onclick: run,
                "Run query"
            }
        }

        if let Some(msg) = error.read().clone() {
            div { class: "mt-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "Query error: {msg}"
            }
        }

        if *loading.read() {
            div { class: "mt-4 p-4 text-center text-obsidian-text-muted text-sm", "Running…" }
        } else if *has_run.read() {
            {
                let out = results.read().clone();
                if out.is_empty() {
                    rsx! {
                        div { class: "mt-4 p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                            "No transactions match this query."
                        }
                    }
                } else {
                    rsx! {
                        div { class: "mt-4 mb-2 text-xs text-obsidian-text-muted", "{out.len()} result(s)" }
                        div { class: "space-y-2",
                            for txn in out {
                                TransactionListRow {
                                    key: "{txn.id}",
                                    txn: txn.clone(),
                                    on_click: {
                                        let id = txn.id.clone();
                                        let handler = on_open_txn;
                                        move |_| handler.call(id.clone())
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Filter inputs above the list. `draft` is held by the parent so typing
/// doesn't fire requests; only Apply copies into the parent's `active_filter`
/// signal. Clear resets both the inputs and the active filter.
#[component]
fn FilterBar(
    draft: Signal<TxnFilter>,
    on_apply: EventHandler<TxnFilter>,
    on_clear: EventHandler<()>,
    filter_active: bool,
) -> Element {
    let input_class = "px-2 py-1.5 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-xs outline-none focus:border-obsidian-accent placeholder-obsidian-text-muted";
    let date_val_from = draft.read().date_from.clone().unwrap_or_default();
    let date_val_to = draft.read().date_to.clone().unwrap_or_default();
    let account_val = draft.read().account.clone().unwrap_or_default();
    let category_val = draft.read().category.clone().unwrap_or_default();
    let tag_val = draft.read().tag.clone().unwrap_or_default();

    rsx! {
        div { class: "mb-4 p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg",
            div { class: "flex flex-wrap items-end gap-2",
                label { class: "flex flex-col gap-1",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "From" }
                    input {
                        r#type: "date",
                        class: "{input_class}",
                        value: "{date_val_from}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.date_from = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "To" }
                    input {
                        r#type: "date",
                        class: "{input_class}",
                        value: "{date_val_to}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.date_to = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1 flex-1 min-w-[140px]",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "Account contains" }
                    input {
                        r#type: "text",
                        placeholder: "e.g. Groceries",
                        class: "{input_class}",
                        value: "{account_val}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.account = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1 flex-1 min-w-[120px]",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "Category" }
                    input {
                        r#type: "text",
                        placeholder: "exact",
                        class: "{input_class}",
                        value: "{category_val}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.category = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1 flex-1 min-w-[120px]",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "Tag" }
                    input {
                        r#type: "text",
                        placeholder: "exact",
                        class: "{input_class}",
                        value: "{tag_val}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.tag = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                div { class: "flex gap-2 items-center",
                    button {
                        class: "px-3 py-1.5 bg-obsidian-accent text-black text-xs font-semibold rounded-md hover:opacity-90 transition-opacity",
                        r#type: "button",
                        onclick: move |_| on_apply.call(draft.read().clone()),
                        "Apply"
                    }
                    if filter_active {
                        button {
                            class: "px-3 py-1.5 text-xs text-obsidian-text-muted hover:text-obsidian-text underline",
                            r#type: "button",
                            onclick: move |_| on_clear.call(()),
                            "Clear"
                        }
                    }
                }
            }
        }
    }
}

/// One row in the transaction list. Ledger-style: every posting line is
/// shown so the user sees exactly where money moved. Header carries
/// description + all four state signals (category, tags, cleared,
/// attachment) and wraps naturally on narrow screens so the description
/// never truncates. Clicking anywhere on the row routes to the detail view;
/// category / tag chips are interactive (inline edit, stop_propagation).
#[component]
fn TransactionListRow(txn: TransactionView, on_click: EventHandler<()>) -> Element {
    // Local mutable mirror of the prop so inline edits render optimistically
    // without waiting for a parent refetch. Backend errors revert the change
    // via the error toast on the chip itself.
    let local = use_signal(|| txn.clone());
    let snapshot = local.read().clone();
    let postings = posting_views(&snapshot.postings);
    let has_attachment = snapshot.attachment.is_some();
    let txn_id = snapshot.id.clone();

    let on_save_category = {
        let id = txn_id.clone();
        let mut local = local;
        move |value: String| {
            let id = id.clone();
            spawn(async move {
                if let Err(e) = bridge::invoke_categorize_transaction(&id, &value).await {
                    web_sys::console::error_1(&format!("categorize failed: {e}").into());
                    return;
                }
                let mut next = local.read().clone();
                next.category = if value.is_empty() { None } else { Some(value) };
                local.set(next);
            });
        }
    };

    let on_save_tags = {
        let id = txn_id.clone();
        let mut local = local;
        move |tags: Vec<String>| {
            let id = id.clone();
            let tags_clone = tags.clone();
            spawn(async move {
                if let Err(e) = bridge::invoke_tag_transaction(&id, tags_clone).await {
                    web_sys::console::error_1(&format!("tag failed: {e}").into());
                    return;
                }
                let mut next = local.read().clone();
                next.tags_top = tags;
                local.set(next);
            });
        }
    };

    rsx! {
        button {
            r#type: "button",
            class: "block w-full text-left p-3 bg-obsidian-sidebar/60 border border-white/5 rounded-lg hover:border-obsidian-accent/40 transition-colors",
            onclick: move |_| on_click.call(()),
            // Header — flex-wrap so trailing chips/icons spill to a second
            // line on mobile rather than pushing description off-screen.
            div { class: "flex flex-wrap items-center gap-x-2 gap-y-1 mb-2",
                span { class: "text-xs text-obsidian-text-muted font-mono shrink-0",
                    "{snapshot.date}"
                }
                span { class: "text-sm text-obsidian-text", "{snapshot.description}" }
                EditableCategoryChip {
                    current: snapshot.category.clone(),
                    on_save: on_save_category,
                }
                EditableTagList {
                    current: snapshot.tags_top.clone(),
                    on_save: on_save_tags,
                }
                if snapshot.cleared {
                    span {
                        class: "text-xs text-emerald-400",
                        title: "Reconciled against a statement",
                        "✓"
                    }
                }
                if has_attachment {
                    svg {
                        class: "w-3.5 h-3.5 text-obsidian-text-muted",
                        fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        title { "Has attachment" }
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                            d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13"
                        }
                    }
                }
            }
            // Postings — one row per leg, account left-truncated, amount
            // right-aligned. Mono font keeps decimals visually stacked.
            div { class: "space-y-0.5 pl-3",
                for posting in postings {
                    div { class: "flex justify-between gap-3 text-xs font-mono",
                        span { class: "text-obsidian-text-muted truncate",
                            "{posting.account}"
                        }
                        span { class: "text-obsidian-text shrink-0",
                            "{posting.amount} {posting.commodity}"
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Transaction detail view (Phase 4.2)
//
// Read-only view of one transaction's full payload plus an attachment viewer.
// Edit (category + tag) lands in Phase 4.3. Attachment rendering uses
// `URL.createObjectURL(Blob)` so large PDFs don't bloat the DOM via base64
// data URIs; the URL is revoked when the component unmounts via a guard.
// =============================================================================

/// Owns a blob: URL for an attachment and revokes it on Drop so the WebView
/// doesn't accumulate orphaned object URLs across navigations. Holding this
/// inside a `Signal<Option<ObjectUrlGuard>>` ties the URL's lifetime to the
/// detail view component.
struct ObjectUrlGuard(String);

impl ObjectUrlGuard {
    fn from_bytes(bytes: &[u8], mime: &str) -> Result<Self, String> {
        use wasm_bindgen::JsCast;
        let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        arr.copy_from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&arr.buffer());
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
            parts.unchecked_ref(),
            &opts,
        )
        .map_err(|e| format!("blob construct: {e:?}"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)
            .map_err(|e| format!("object url: {e:?}"))?;
        Ok(Self(url))
    }

    fn url(&self) -> &str {
        &self.0
    }
}

impl Drop for ObjectUrlGuard {
    fn drop(&mut self) {
        let _ = web_sys::Url::revoke_object_url(&self.0);
    }
}

/// What kind of inline rendering the attachment supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttachmentRender {
    Image,
    Pdf,
    Other,
}

fn classify_attachment(mime: &str) -> AttachmentRender {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("image/") {
        AttachmentRender::Image
    } else if mime == "application/pdf" || mime == "application/x-pdf" {
        AttachmentRender::Pdf
    } else {
        AttachmentRender::Other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachmentMeta {
    sha256: String,
    filename: String,
    mime_type: String,
    size: u64,
}

/// Pull the four `AttachmentRef` fields out of the serde_json::Value the
/// backend ships. Returns None when the field is null or any required key
/// is missing — defensive only; `record_transaction` validates upstream.
fn extract_attachment_meta(value: &serde_json::Value) -> Option<AttachmentMeta> {
    Some(AttachmentMeta {
        sha256: value.get("sha256")?.as_str()?.to_string(),
        filename: value.get("filename")?.as_str()?.to_string(),
        mime_type: value.get("mime_type")?.as_str()?.to_string(),
        size: value.get("size")?.as_u64().unwrap_or(0),
    })
}

#[component]
fn TransactionDetailView(txn_id: String, on_back: EventHandler<()>) -> Element {
    let mut txn: Signal<Option<Result<TransactionView, String>>> = use_signal(|| None);
    let txn_id_for_load = txn_id.clone();

    use_effect(move || {
        let id = txn_id_for_load.clone();
        spawn(async move {
            let res = bridge::invoke_get_transaction(&id).await;
            let mapped = match res {
                Ok(Some(t)) => Ok(t),
                Ok(None) => Err(format!(
                    "Transaction {id} not found. It may have been deleted or merged on another device."
                )),
                Err(e) => Err(e),
            };
            txn.set(Some(mapped));
        });
    });

    let header = rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Transaction"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← List"
            }
        }
    };

    match txn.read().clone() {
        None => rsx! {
            {header}
            div { class: "text-obsidian-text-muted text-sm", "Loading…" }
        },
        Some(Err(msg)) => rsx! {
            {header}
            div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        },
        Some(Ok(t)) => rsx! {
            {header}
            TransactionDetailBody { txn: t }
        },
    }
}

#[component]
fn TransactionDetailBody(txn: TransactionView) -> Element {
    // Local mirror of the prop for optimistic-edit updates. Same pattern as
    // TransactionListRow — backend writes succeed before the projection
    // refresh would otherwise re-render us.
    let local = use_signal(|| txn.clone());
    let snapshot = local.read().clone();
    let postings = posting_views(&snapshot.postings);
    let attachment_meta = snapshot.attachment.as_ref().and_then(extract_attachment_meta);
    let txn_id = snapshot.id.clone();

    let on_save_category = {
        let id = txn_id.clone();
        let mut local = local;
        move |value: String| {
            let id = id.clone();
            spawn(async move {
                if let Err(e) = bridge::invoke_categorize_transaction(&id, &value).await {
                    web_sys::console::error_1(&format!("categorize failed: {e}").into());
                    return;
                }
                let mut next = local.read().clone();
                next.category = if value.is_empty() { None } else { Some(value) };
                local.set(next);
            });
        }
    };

    let on_save_tags = {
        let id = txn_id.clone();
        let mut local = local;
        move |tags: Vec<String>| {
            let id = id.clone();
            let tags_clone = tags.clone();
            spawn(async move {
                if let Err(e) = bridge::invoke_tag_transaction(&id, tags_clone).await {
                    web_sys::console::error_1(&format!("tag failed: {e}").into());
                    return;
                }
                let mut next = local.read().clone();
                next.tags_top = tags;
                local.set(next);
            });
        }
    };

    rsx! {
        div { class: "space-y-6",
            // --- Metadata block ---
            div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
                div { class: "flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-3",
                    span { class: "text-sm text-obsidian-text-muted font-mono",
                        "{snapshot.date}"
                    }
                    h2 { class: "text-lg font-semibold text-obsidian-text",
                        "{snapshot.description}"
                    }
                }
                div { class: "flex flex-wrap gap-2 items-center",
                    EditableCategoryChip {
                        current: snapshot.category.clone(),
                        on_save: on_save_category,
                    }
                    EditableTagList {
                        current: snapshot.tags_top.clone(),
                        on_save: on_save_tags,
                    }
                    if snapshot.cleared {
                        span { class: "text-xs px-2 py-0.5 bg-emerald-500/15 text-emerald-300 rounded-full",
                            "✓ Cleared"
                            if let Some(date) = snapshot.cleared_date.clone() {
                                " · {date}"
                            }
                        }
                    }
                }
                if let Some(src) = snapshot.statement_source.clone() {
                    div { class: "mt-3 text-xs text-obsidian-text-muted",
                        "Statement: "
                        span { class: "font-mono", "{src}" }
                    }
                }
            }

            // --- Postings ---
            div {
                h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                    "Postings"
                }
                div { class: "space-y-1 p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg",
                    for posting in postings {
                        div { class: "flex justify-between gap-3 text-sm font-mono",
                            span { class: "text-obsidian-text-muted truncate",
                                "{posting.account}"
                            }
                            span { class: "text-obsidian-text shrink-0",
                                "{posting.amount} {posting.commodity}"
                            }
                        }
                    }
                }
            }

            // --- Attachment ---
            if let Some(meta) = attachment_meta {
                AttachmentViewer { meta: meta }
            }
        }
    }
}

#[component]
fn AttachmentViewer(meta: AttachmentMeta) -> Element {
    let mut url_guard: Signal<Option<ObjectUrlGuard>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let sha256 = meta.sha256.clone();
    let mime = meta.mime_type.clone();
    use_effect(move || {
        let sha = sha256.clone();
        let mime = mime.clone();
        spawn(async move {
            match bridge::invoke_fetch_attachment(&sha).await {
                Ok(bytes) => match ObjectUrlGuard::from_bytes(&bytes, &mime) {
                    Ok(guard) => url_guard.set(Some(guard)),
                    Err(e) => error.set(Some(e)),
                },
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let render = classify_attachment(&meta.mime_type);
    let size_kb = (meta.size as f64 / 1024.0).round() as u64;

    rsx! {
        div {
            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                "Attachment"
            }
            div { class: "p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg space-y-3",
                div { class: "flex items-center gap-2 text-xs text-obsidian-text-muted",
                    svg { class: "w-3.5 h-3.5 text-obsidian-accent",
                        fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                            d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13"
                        }
                    }
                    span { class: "font-mono text-obsidian-text", "{meta.filename}" }
                    span { " · {size_kb} KB · {meta.mime_type}" }
                }

                if let Some(msg) = error.read().clone() {
                    div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-xs text-red-300",
                        "Couldn't load attachment: {msg}"
                    }
                } else {
                    match url_guard.read().as_ref().map(|g| g.url().to_string()) {
                        None => rsx! {
                            div { class: "text-xs text-obsidian-text-muted", "Loading attachment…" }
                        },
                        Some(url) => match render {
                            AttachmentRender::Image => rsx! {
                                img {
                                    src: "{url}",
                                    alt: "{meta.filename}",
                                    class: "max-w-full max-h-[600px] rounded border border-white/10",
                                }
                            },
                            AttachmentRender::Pdf => rsx! {
                                iframe {
                                    src: "{url}",
                                    class: "w-full h-[600px] rounded border border-white/10 bg-white",
                                    title: "{meta.filename}",
                                }
                            },
                            AttachmentRender::Other => rsx! {
                                a {
                                    href: "{url}",
                                    download: "{meta.filename}",
                                    class: "inline-block px-3 py-1.5 text-xs bg-obsidian-accent text-black font-medium rounded hover:opacity-90",
                                    "Download {meta.filename}"
                                }
                            },
                        },
                    }
                }
            }
        }
    }
}

// =============================================================================
// Phase 4.4 — Accounts screen
// =============================================================================

/// Format a stringified Decimal balance for display:
///   "1234.5" + "CAD" → "1,234.50 CAD"
///   "-1450.18" + "CAD" → "-1,450.18 CAD"
///
/// Pure formatting on string inputs (the wire shape) — no rust_decimal
/// dep in the frontend. Pads to 2 decimal places (matches every-day money
/// display); accepts more decimal places verbatim (e.g. BTC). Adds
/// thousands separators on the integer part.
fn format_money(amount: &str, commodity: &str) -> String {
    let (sign, magnitude) = match amount.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", amount),
    };
    let (int_part, frac_part) = match magnitude.split_once('.') {
        Some((i, f)) => (i, f.to_string()),
        None => (magnitude, String::new()),
    };
    let frac_padded = if frac_part.len() < 2 {
        format!("{frac_part:0<2}")
    } else {
        frac_part
    };
    let int_grouped = group_thousands(int_part);
    format!("{sign}{int_grouped}.{frac_padded} {commodity}")
}

fn group_thousands(int_part: &str) -> String {
    let bytes = int_part.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

#[component]
fn AccountListView(on_back: EventHandler<()>) -> Element {
    let mut summaries: Signal<Vec<AccountSummaryView>> = use_signal(Vec::new);
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(None);
            match bridge::invoke_account_summaries(None).await {
                Ok(rows) => summaries.set(rows),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    let rows = summaries.read().clone();
    let is_loading = *loading.read();
    let err_msg = error.read().clone();
    let show_empty = rows.is_empty() && !is_loading && err_msg.is_none();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Accounts"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "Failed to load accounts: {msg}"
            }
        }

        if is_loading {
            div { class: "p-6 text-center text-obsidian-text-muted text-sm",
                "Loading…"
            }
        } else if show_empty {
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                "No accounts to show yet. Record a transaction or declare an account from Settings to populate this screen."
            }
        } else {
            div { class: "space-y-3",
                for summary in rows {
                    AccountSummaryCard {
                        key: "{summary.account}",
                        summary: summary,
                    }
                }
            }
        }
    }
}

#[component]
fn AccountSummaryCard(summary: AccountSummaryView) -> Element {
    let header_label = summary
        .display_name
        .clone()
        .unwrap_or_else(|| summary.account.clone());
    let sub_label = if summary.display_name.is_some() {
        Some(summary.account.clone())
    } else {
        None
    };

    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "flex items-baseline justify-between gap-3 mb-3",
                div { class: "min-w-0",
                    div { class: "text-sm font-semibold text-obsidian-text truncate",
                        "{header_label}"
                    }
                    if let Some(sub) = sub_label {
                        div { class: "text-xs text-obsidian-text-muted truncate", "{sub}" }
                    }
                }
                match summary.total_in_base.as_deref() {
                    Some(total) => rsx! {
                        div { class: "text-sm font-mono text-obsidian-text shrink-0",
                            "{format_money(total, \"CAD\")}"
                        }
                    },
                    None => rsx! {
                        div { class: "text-sm font-mono text-obsidian-text-muted shrink-0", "—" }
                    },
                }
            }

            if !summary.balances.is_empty() {
                div { class: "space-y-1 border-t border-white/5 pt-2",
                    for bal in summary.balances.iter() {
                        div { class: "flex items-baseline justify-between text-xs text-obsidian-text-muted font-mono",
                            span { "{format_money(&bal.quantity, &bal.commodity)}" }
                            match bal.value_in_base.as_deref() {
                                Some(v) if bal.commodity != "CAD" => rsx! {
                                    span { class: "text-obsidian-text-muted/70",
                                        "≈ {format_money(v, \"CAD\")}"
                                    }
                                },
                                _ => rsx! { span {} },
                            }
                        }
                    }
                }
            }

            if let Some(date) = summary.last_reconciled_through.as_deref() {
                div { class: "mt-3 text-xs text-obsidian-text-muted",
                    "Last reconciled through {date}"
                    if let Some(bal) = summary.last_statement_balance.as_deref() {
                        " · statement {format_money(bal, \"CAD\")}"
                    }
                }
            }
        }
    }
}

// =============================================================================
// Phase 4.5 + 4.6 — R1 financial-health glance dashboard
// =============================================================================

/// Largest signed `(income, spending)` figure across all months — used to
/// normalize the trend chart's bar heights. Returns `None` when every
/// month is exactly zero (an entirely empty trend renders flat).
fn max_trend_magnitude(buckets: &[MonthlyTrendBucketView]) -> Option<f64> {
    let max = buckets
        .iter()
        .flat_map(|b| {
            [
                b.income.parse::<f64>().unwrap_or(0.0).abs(),
                b.spending.parse::<f64>().unwrap_or(0.0).abs(),
            ]
        })
        .fold(0.0_f64, f64::max);
    if max <= 0.0 {
        None
    } else {
        Some(max)
    }
}

/// Project a numeric string + scale onto a 0-100% bar height. Defensive on
/// parse failure (returns 0).
fn bar_height_pct(amount: &str, scale: f64) -> f64 {
    if scale <= 0.0 {
        return 0.0;
    }
    let v = amount.parse::<f64>().unwrap_or(0.0).abs();
    ((v / scale) * 100.0).clamp(0.0, 100.0)
}

/// Pretty-print a cadence in days as a human label.
/// 30 → "monthly", 14 → "biweekly", 7 → "weekly", else "every N days".
fn cadence_label(days: u32) -> String {
    match days {
        7 => "weekly".to_string(),
        14 => "biweekly".to_string(),
        30 | 31 => "monthly".to_string(),
        0 => "—".to_string(),
        n => format!("every {n} days"),
    }
}

#[component]
fn DashboardView(
    on_back: EventHandler<()>,
    on_open_unmatched: EventHandler<()>,
) -> Element {
    let mut summary: Signal<Option<DashboardSummaryView>> = use_signal(|| None);
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error.set(None);
            match bridge::invoke_dashboard_summary(None).await {
                Ok(s) => summary.set(Some(s)),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    let snapshot = summary.read().clone();
    let is_loading = *loading.read();
    let err_msg = error.read().clone();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Dashboard"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "Failed to load dashboard: {msg}"
            }
        }

        if is_loading && snapshot.is_none() {
            div { class: "p-6 text-center text-obsidian-text-muted text-sm", "Loading…" }
        } else if let Some(s) = snapshot {
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                NetWorthCard {
                    net_worth: s.net_worth_in_base.clone(),
                    base_currency: s.base_currency.clone(),
                }
                UnmatchedCard {
                    unmatched: s.unmatched_balance.clone(),
                    base_currency: s.base_currency.clone(),
                    on_click: move |_| on_open_unmatched.call(()),
                }
            }
            MonthlyTrendCard {
                buckets: s.monthly_buckets.clone(),
                base_currency: s.base_currency.clone(),
            }
            RecurringCard {
                recurring: s.recurring.clone(),
                base_currency: s.base_currency.clone(),
            }
            AffordCard {
                base_currency: s.base_currency.clone(),
            }
        }
    }
}

#[component]
fn NetWorthCard(net_worth: Option<String>, base_currency: String) -> Element {
    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "text-xs text-obsidian-text-muted uppercase tracking-wider mb-1",
                "Net worth"
            }
            match net_worth {
                Some(v) => rsx! {
                    div { class: "text-2xl font-bold text-obsidian-text font-mono",
                        "{format_money(&v, &base_currency)}"
                    }
                },
                None => rsx! {
                    div { class: "text-2xl font-bold text-obsidian-text-muted", "—" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Declare an account or record a transaction to populate this."
                    }
                },
            }
        }
    }
}

#[component]
fn UnmatchedCard(
    unmatched: Option<String>,
    base_currency: String,
    on_click: EventHandler<()>,
) -> Element {
    // Treat exactly-zero as nothing to show; non-zero is the
    // reconciliation-pending signal that earns the orange accent.
    let is_pending = unmatched
        .as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .is_some_and(|v| v.abs() > 0.0);
    let border = if is_pending {
        "border-amber-500/40 hover:border-amber-400/60"
    } else {
        "border-white/10 hover:border-obsidian-accent/40"
    };

    rsx! {
        button {
            class: "p-4 bg-obsidian-sidebar/60 border rounded-lg text-left w-full transition-colors {border}",
            onclick: move |_| on_click.call(()),
            div { class: "text-xs text-obsidian-text-muted uppercase tracking-wider mb-1",
                "Unmatched balance"
            }
            match unmatched {
                Some(v) => rsx! {
                    div { class: "text-2xl font-bold font-mono",
                        class: if is_pending { "text-amber-300" } else { "text-obsidian-text" },
                        "{format_money(&v, &base_currency)}"
                    }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        if is_pending {
                            "Reconciliation pending — tap to review unmatched transactions."
                        } else {
                            "Steady-state zero. Everything reconciles."
                        }
                    }
                },
                None => rsx! {
                    div { class: "text-2xl font-bold text-obsidian-text-muted", "—" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "No unmatched activity to clear."
                    }
                },
            }
        }
    }
}

#[component]
fn MonthlyTrendCard(buckets: Vec<MonthlyTrendBucketView>, base_currency: String) -> Element {
    let scale = max_trend_magnitude(&buckets);
    rsx! {
        div { class: "mt-3 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "flex items-baseline justify-between mb-3",
                div { class: "text-xs text-obsidian-text-muted uppercase tracking-wider",
                    "Income vs. spending — last {buckets.len()} months"
                }
                div { class: "flex items-center gap-3 text-[10px] text-obsidian-text-muted",
                    span { class: "flex items-center gap-1",
                        span { class: "inline-block w-2 h-2 bg-emerald-500/70 rounded-sm" }
                        "income"
                    }
                    span { class: "flex items-center gap-1",
                        span { class: "inline-block w-2 h-2 bg-rose-500/70 rounded-sm" }
                        "spending"
                    }
                }
            }
            match scale {
                None => rsx! {
                    div { class: "text-sm text-obsidian-text-muted text-center py-6",
                        "No income or spending in the trend window yet."
                    }
                },
                Some(s) => rsx! {
                    div { class: "flex items-stretch gap-2 h-32",
                        for bucket in buckets.iter() {
                            div { class: "flex-1 flex flex-col items-center gap-1 min-h-0",
                                div { class: "flex-1 w-full flex items-end gap-0.5 min-h-0",
                                    div { class: "flex-1 bg-emerald-500/70 rounded-sm",
                                        style: "height: {bar_height_pct(&bucket.income, s)}%",
                                        title: "{format_money(&bucket.income, &base_currency)} income",
                                    }
                                    div { class: "flex-1 bg-rose-500/70 rounded-sm",
                                        style: "height: {bar_height_pct(&bucket.spending, s)}%",
                                        title: "{format_money(&bucket.spending, &base_currency)} spending",
                                    }
                                }
                                div { class: "text-[10px] text-obsidian-text-muted font-mono",
                                    "{bucket.month}"
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn RecurringCard(recurring: Vec<RecurringObligationView>, base_currency: String) -> Element {
    rsx! {
        div { class: "mt-3 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "text-xs text-obsidian-text-muted uppercase tracking-wider mb-3",
                "Recurring obligations"
            }
            if recurring.is_empty() {
                div { class: "text-sm text-obsidian-text-muted text-center py-4",
                    "No confirmed recurring patterns yet. Phase 5.3 detection scanner will populate this."
                }
            } else {
                div { class: "space-y-2",
                    for r in recurring.iter() {
                        div { class: "flex items-baseline justify-between text-sm",
                            div { class: "min-w-0",
                                span { class: "text-obsidian-text font-medium", "{r.vendor}" }
                                span { class: "text-obsidian-text-muted text-xs ml-2",
                                    "{cadence_label(r.cadence_days)}"
                                }
                            }
                            span { class: "font-mono text-obsidian-text shrink-0",
                                "{format_money(&r.amount, &r.commodity)}"
                            }
                        }
                    }
                }
            }
            // Show the base currency hint only when at least one obligation
            // is in a non-base commodity — keeps the card uncluttered for
            // the all-CAD common case.
            if recurring.iter().any(|r| !r.commodity.eq_ignore_ascii_case(&base_currency)) {
                div { class: "text-[10px] text-obsidian-text-muted mt-2",
                    "Base currency: {base_currency}"
                }
            }
        }
    }
}

#[component]
fn AffordCard(base_currency: String) -> Element {
    let mut amount: Signal<String> = use_signal(String::new);
    let mut verdict: Signal<Option<AffordVerdictView>> = use_signal(|| None);
    let mut loading: Signal<bool> = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let submit = move |e: FormEvent| {
        e.prevent_default();
        let raw = amount.read().clone();
        if raw.trim().is_empty() {
            return;
        }
        loading.set(true);
        error.set(None);
        spawn(async move {
            match bridge::invoke_check_affordability(raw.trim(), None).await {
                Ok(v) => {
                    verdict.set(Some(v));
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    let current_verdict = verdict.read().clone();
    let err_msg = error.read().clone();

    rsx! {
        div { class: "mt-3 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "text-xs text-obsidian-text-muted uppercase tracking-wider mb-3",
                "Can I afford …"
            }
            form {
                class: "flex gap-2 items-stretch",
                onsubmit: submit,
                div { class: "flex items-center bg-obsidian-bg/60 border border-white/10 rounded px-3 flex-1",
                    span { class: "text-obsidian-text-muted text-sm mr-2", "{base_currency}" }
                    input {
                        class: "bg-transparent text-obsidian-text text-sm w-full py-2 focus:outline-none",
                        r#type: "text",
                        placeholder: "Amount",
                        value: "{amount.read()}",
                        oninput: move |e| amount.set(e.value().clone()),
                    }
                }
                button {
                    class: "px-4 py-2 bg-obsidian-accent text-black text-sm font-medium rounded hover:opacity-90 disabled:opacity-50",
                    r#type: "submit",
                    disabled: *loading.read(),
                    if *loading.read() { "Checking…" } else { "Check" }
                }
            }

            if let Some(msg) = err_msg {
                div { class: "mt-3 p-2 bg-red-950/30 border border-red-500/30 rounded text-xs text-red-300",
                    "{msg}"
                }
            }

            if let Some(v) = current_verdict {
                div { class: "mt-3 p-3 border rounded",
                    class: if v.can_afford {
                        "bg-emerald-950/30 border-emerald-500/40"
                    } else {
                        "bg-rose-950/30 border-rose-500/40"
                    },
                    div { class: "flex items-baseline justify-between gap-3",
                        div { class: "text-sm font-semibold",
                            class: if v.can_afford { "text-emerald-300" } else { "text-rose-300" },
                            if v.can_afford { "Yes — you'd have " } else { "No — you'd be at " }
                            span { class: "font-mono",
                                "{format_money(&v.remaining_in_base, &v.base_currency)}"
                            }
                        }
                    }
                    div { class: "text-[11px] text-obsidian-text-muted mt-1",
                        "Policy: {v.policy_label}"
                    }
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Budgets (Phase 5.1) — W4 setup screen.
// -----------------------------------------------------------------------------

/// Cadence options exposed in the period `<select>`. `"custom:N"` is supported
/// by the event schema but not surfaced here — sticking to the three named
/// cadences keeps the picker honest with what the spec asked for. If a custom
/// budget arrives from another source (sync, hand-edited event log), the list
/// row displays the raw string so the user can at least see it.
const PERIOD_CHOICES: &[(&str, &str)] = &[
    ("monthly", "Monthly"),
    ("biweekly", "Biweekly"),
    ("weekly", "Weekly"),
];

/// Human label for a period string. Falls back to the raw value for anything
/// the picker doesn't surface (e.g., a hand-crafted `"custom:90"`).
fn period_label(period: &str) -> String {
    PERIOD_CHOICES
        .iter()
        .find(|(value, _)| *value == period)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| period.to_string())
}

/// Trim a category input and reject if empty. Returns `Some(trimmed)` if the
/// trimmed string is non-empty.
fn normalize_category(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Validate an amount string — must parse as a finite positive number.
/// Returns the trimmed canonical string on success, or an error message on
/// failure. Kept lax (f64-parse) to match the rest of the manual-entry
/// surface; the event store accepts any decimal string and the projection
/// keeps it verbatim.
fn validate_amount(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Amount is required");
    }
    let parsed: f64 = trimmed.parse().map_err(|_| "Amount must be a number")?;
    if !parsed.is_finite() {
        return Err("Amount must be finite");
    }
    if parsed <= 0.0 {
        return Err("Amount must be greater than zero");
    }
    Ok(trimmed.to_string())
}

/// Tailwind class for a progress-bar fill based on `over_budget` + raw
/// percent. Three tiers — green under 75%, amber 75-100%, red over budget.
/// Picks a class string rather than a color value so the existing Tailwind
/// purge sees them at build time.
fn progress_color_class(percent_used: f64, over_budget: bool) -> &'static str {
    if over_budget {
        "bg-red-500/70"
    } else if percent_used >= 75.0 {
        "bg-amber-400/70"
    } else {
        "bg-emerald-500/70"
    }
}

/// CSS-`width` string for the progress bar's fill — visually clamps at
/// 100% so an over-budget bar doesn't overflow its container, while the
/// numeric label below the bar still shows the true percentage.
fn progress_width_pct(percent_used: f64) -> f64 {
    percent_used.clamp(0.0, 100.0)
}

/// Compact "May 1 – May 31" label for the period range. Used in the
/// per-row progress sub-line.
fn period_range_label(start: &str, end: &str) -> String {
    format!("{start} – {end}")
}

#[component]
fn BudgetListView(on_back: EventHandler<()>) -> Element {
    let mut rows: Signal<Vec<BudgetRow>> = use_signal(Vec::new);
    let mut progress: Signal<Vec<BudgetProgress>> = use_signal(Vec::new);
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);

    // Add/edit form state. `editing_category` is `Some(category)` when the
    // form is editing an existing row (category field locked); `None` means
    // a fresh add. Inputs stay as raw strings so the user's typing isn't
    // disturbed by mid-edit reformatting.
    let mut editing_category: Signal<Option<String>> = use_signal(|| None);
    let mut category_input: Signal<String> = use_signal(String::new);
    let mut amount_input: Signal<String> = use_signal(String::new);
    let mut period_input: Signal<String> = use_signal(|| "monthly".to_string());
    let mut form_error: Signal<Option<String>> = use_signal(|| None);
    let mut saving: Signal<bool> = use_signal(|| false);

    let load_rows = move || {
        spawn(async move {
            loading.set(true);
            load_error.set(None);
            match bridge::invoke_list_budgets().await {
                Ok(fetched) => rows.set(fetched),
                Err(e) => load_error.set(Some(e)),
            }
            // Progress is best-effort — if it fails (bad amount string,
            // missing journal, FX gap), the list still renders without
            // bars rather than hiding the budgets behind a load error.
            match bridge::invoke_budget_progress(None).await {
                Ok(fetched) => progress.set(fetched),
                Err(_) => progress.set(Vec::new()),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_rows();
    });

    let mut reset_form = move || {
        editing_category.set(None);
        category_input.set(String::new());
        amount_input.set(String::new());
        period_input.set("monthly".to_string());
        form_error.set(None);
    };

    let mut save = move || {
        let raw_category = category_input.read().clone();
        let raw_amount = amount_input.read().clone();
        let period = period_input.read().clone();
        let editing = editing_category.read().clone();

        // When editing, the category is locked to the existing row's id; the
        // input is read-only and we use the locked value instead.
        let category = match editing {
            Some(ref cat) => cat.clone(),
            None => match normalize_category(&raw_category) {
                Some(c) => c,
                None => {
                    form_error.set(Some("Category is required".to_string()));
                    return;
                }
            },
        };

        let amount = match validate_amount(&raw_amount) {
            Ok(a) => a,
            Err(msg) => {
                form_error.set(Some(msg.to_string()));
                return;
            }
        };

        form_error.set(None);
        saving.set(true);
        spawn(async move {
            match bridge::invoke_set_budget(&category, &amount, &period).await {
                Ok(_row) => {
                    reset_form();
                    load_rows();
                }
                Err(e) => form_error.set(Some(format!("Save failed: {e}"))),
            }
            saving.set(false);
        });
    };

    let mut start_edit = move |row: BudgetRow| {
        editing_category.set(Some(row.id.clone()));
        category_input.set(row.id);
        amount_input.set(row.amount);
        period_input.set(row.period);
        form_error.set(None);
    };

    let remove = move |category: String| {
        spawn(async move {
            if let Err(e) = bridge::invoke_remove_budget(&category).await {
                load_error.set(Some(format!("Remove failed: {e}")));
                return;
            }
            // If the form was editing this row, drop the edit context.
            let was_editing = editing_category
                .read()
                .as_ref()
                .map(|c| c == &category)
                .unwrap_or(false);
            if was_editing {
                reset_form();
            }
            load_rows();
        });
    };

    let snapshot = rows.read().clone();
    let is_loading = *loading.read();
    let err_msg = load_error.read().clone();
    let form_err = form_error.read().clone();
    let is_saving = *saving.read();
    let editing = editing_category.read().clone();
    let editing_some = editing.is_some();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Budgets"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        // --- Add / edit form ---
        div { class: "mb-6 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "flex items-center justify-between mb-3",
                div { class: "text-sm font-semibold text-obsidian-text",
                    if editing_some {
                        "Edit budget"
                    } else {
                        "Add a budget"
                    }
                }
                if editing_some {
                    button {
                        class: "text-xs text-obsidian-text-muted hover:text-obsidian-text underline",
                        onclick: move |_| reset_form(),
                        "Cancel edit"
                    }
                }
            }

            div { class: "space-y-3",
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1",
                        "Category"
                    }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none disabled:opacity-60",
                        r#type: "text",
                        placeholder: "Expenses:Groceries",
                        value: "{category_input.read()}",
                        disabled: editing_some,
                        oninput: move |e| category_input.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-2 gap-3",
                    div {
                        label { class: "block text-xs text-obsidian-text-muted mb-1",
                            "Target amount"
                        }
                        input {
                            class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none",
                            r#type: "text",
                            inputmode: "decimal",
                            placeholder: "0.00",
                            value: "{amount_input.read()}",
                            oninput: move |e| amount_input.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-xs text-obsidian-text-muted mb-1",
                            "Cycle"
                        }
                        select {
                            class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                            value: "{period_input.read()}",
                            onchange: move |e| period_input.set(e.value()),
                            for (value, label) in PERIOD_CHOICES.iter() {
                                option { value: "{value}", "{label}" }
                            }
                        }
                    }
                }

                if let Some(msg) = form_err {
                    div { class: "text-xs text-red-300 px-1", "{msg}" }
                }

                div { class: "flex justify-end gap-2 pt-1",
                    button {
                        class: "px-4 py-2 bg-obsidian-accent/90 hover:bg-obsidian-accent text-black text-sm font-semibold rounded disabled:opacity-50",
                        disabled: is_saving,
                        onclick: move |_| save(),
                        if is_saving {
                            "Saving…"
                        } else if editing_some {
                            "Save changes"
                        } else {
                            "Add budget"
                        }
                    }
                }
            }
        }

        // --- Existing budgets list ---
        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        }

        if is_loading {
            div { class: "p-6 text-center text-obsidian-text-muted text-sm",
                "Loading…"
            }
        } else if snapshot.is_empty() {
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                "No budgets yet. Add one above to start tracking a category target."
            }
        } else {
            div { class: "space-y-2",
                {
                    let progress_snapshot = progress.read().clone();
                    rsx! {
                        for row in snapshot {
                            BudgetRowCard {
                                key: "{row.id}",
                                row: row.clone(),
                                progress: progress_snapshot.iter().find(|p| p.category == row.id).cloned(),
                                on_edit: {
                                    let r = row.clone();
                                    move |_| start_edit(r.clone())
                                },
                                on_remove: {
                                    let cat = row.id.clone();
                                    move |_| remove(cat.clone())
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BudgetRowCard(
    row: BudgetRow,
    progress: Option<BudgetProgress>,
    on_edit: EventHandler<()>,
    on_remove: EventHandler<()>,
) -> Element {
    let period_text = period_label(&row.period);
    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex flex-col gap-3",
            div { class: "flex items-center justify-between gap-3",
                div { class: "min-w-0 flex-1",
                    div { class: "text-sm font-semibold text-obsidian-text truncate",
                        "{row.id}"
                    }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "{row.amount} · {period_text}"
                    }
                }
                div { class: "flex gap-2 shrink-0",
                    button {
                        class: "px-3 py-1.5 text-xs text-obsidian-text-muted hover:text-obsidian-accent border border-white/10 hover:border-obsidian-accent/40 rounded",
                        onclick: move |_| on_edit.call(()),
                        "Edit"
                    }
                    button {
                        class: "px-3 py-1.5 text-xs text-red-300/80 hover:text-red-300 border border-red-500/20 hover:border-red-500/40 rounded",
                        onclick: move |_| on_remove.call(()),
                        "Remove"
                    }
                }
            }
            if let Some(p) = progress {
                BudgetProgressBar { progress: p }
            }
        }
    }
}

#[component]
fn BudgetProgressBar(progress: BudgetProgress) -> Element {
    let width = progress_width_pct(progress.percent_used);
    let color = progress_color_class(progress.percent_used, progress.over_budget);
    let verdict = if progress.over_budget {
        format!("Over by {:.0}%", progress.percent_used - 100.0)
    } else {
        format!("{:.0}% used", progress.percent_used)
    };
    rsx! {
        div {
            div { class: "w-full h-2 bg-obsidian-bg rounded-full overflow-hidden",
                div {
                    class: "{color} h-full transition-all",
                    style: "width: {width}%",
                }
            }
            div { class: "flex items-center justify-between mt-1.5 text-xs text-obsidian-text-muted",
                span { "{progress.actual} of {progress.target} · {verdict}" }
                span { "{period_range_label(&progress.period_start, &progress.period_end)}" }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Recurring review (Phase 5.4) — confirm/dismiss detected patterns.
// -----------------------------------------------------------------------------

/// Format a `cadence_days` integer as a human-readable cadence label.
/// Mirrors the existing `cadence_label` shape but lives in 5.4 so the
/// recurring review can present "monthly" / "weekly" / "every N days"
/// alongside its row data without round-tripping to the dashboard.
fn recurring_cadence_label(cadence_days: u32) -> String {
    match cadence_days {
        7 => "weekly".to_string(),
        14 => "biweekly".to_string(),
        30 | 31 => "monthly".to_string(),
        0 => "—".to_string(),
        n => format!("every {n} days"),
    }
}

#[component]
fn RecurringReviewView(on_back: EventHandler<()>) -> Element {
    let mut rows: Signal<Vec<RecurringPattern>> = use_signal(Vec::new);
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut scanning: Signal<bool> = use_signal(|| false);
    let mut last_scan: Signal<Option<ScanRecurringResult>> = use_signal(|| None);

    let load_rows = move || {
        spawn(async move {
            loading.set(true);
            load_error.set(None);
            match bridge::invoke_list_recurring(Some("detected")).await {
                Ok(fetched) => rows.set(fetched),
                Err(e) => load_error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_rows();
    });

    let scan = move || {
        spawn(async move {
            scanning.set(true);
            match bridge::invoke_scan_recurring(None).await {
                Ok(result) => {
                    last_scan.set(Some(result));
                    load_rows();
                }
                Err(e) => load_error.set(Some(format!("Scan failed: {e}"))),
            }
            scanning.set(false);
        });
    };

    let confirm = move |pattern_id: String| {
        spawn(async move {
            if let Err(e) = bridge::invoke_confirm_recurring(&pattern_id).await {
                load_error.set(Some(format!("Confirm failed: {e}")));
                return;
            }
            load_rows();
        });
    };

    let dismiss = move |pattern_id: String| {
        spawn(async move {
            if let Err(e) = bridge::invoke_dismiss_recurring(&pattern_id).await {
                load_error.set(Some(format!("Dismiss failed: {e}")));
                return;
            }
            load_rows();
        });
    };

    let snapshot = rows.read().clone();
    let is_loading = *loading.read();
    let is_scanning = *scanning.read();
    let err_msg = load_error.read().clone();
    let scan_summary = last_scan.read().clone();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Recurring"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        div { class: "mb-4 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between gap-3",
            div { class: "min-w-0",
                div { class: "text-sm font-semibold text-obsidian-text",
                    "Scan for new patterns"
                }
                div { class: "text-xs text-obsidian-text-muted mt-1",
                    "Sweeps the last year of expense transactions."
                }
                if let Some(s) = scan_summary {
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "Last scan: {s.detected} detected · {s.new_emitted} new · {s.already_tracked} already tracked"
                    }
                }
            }
            button {
                class: "px-4 py-2 bg-obsidian-accent/90 hover:bg-obsidian-accent text-black text-sm font-semibold rounded shrink-0 disabled:opacity-50",
                disabled: is_scanning,
                onclick: move |_| scan(),
                if is_scanning { "Scanning…" } else { "Scan now" }
            }
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        }

        if is_loading {
            div { class: "p-6 text-center text-obsidian-text-muted text-sm",
                "Loading…"
            }
        } else if snapshot.is_empty() {
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                "No detected patterns awaiting review. Run a scan to surface new candidates, or check back after more transactions accumulate."
            }
        } else {
            div { class: "space-y-2",
                for row in snapshot {
                    RecurringRowCard {
                        key: "{row.pattern_id}",
                        row: row.clone(),
                        on_confirm: {
                            let pid = row.pattern_id.clone();
                            move |_| confirm(pid.clone())
                        },
                        on_dismiss: {
                            let pid = row.pattern_id.clone();
                            move |_| dismiss(pid.clone())
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn RecurringRowCard(
    row: RecurringPattern,
    on_confirm: EventHandler<()>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let cadence = recurring_cadence_label(row.cadence_days);
    let span_label = match (row.first_seen.as_deref(), row.last_seen.as_deref()) {
        (Some(f), Some(l)) => format!("{f} → {l}"),
        _ => "—".to_string(),
    };
    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex flex-col gap-3",
            div { class: "min-w-0",
                div { class: "text-sm font-semibold text-obsidian-text truncate",
                    "{row.vendor}"
                }
                div { class: "text-xs text-obsidian-text-muted mt-1",
                    "{row.amount} {row.commodity} · {cadence} · {row.occurrences} occurrences"
                }
                div { class: "text-xs text-obsidian-text-muted",
                    "Seen: {span_label}"
                }
            }
            div { class: "flex gap-2 justify-end",
                button {
                    class: "px-3 py-1.5 text-xs text-red-300/80 hover:text-red-300 border border-red-500/20 hover:border-red-500/40 rounded",
                    onclick: move |_| on_dismiss.call(()),
                    "Dismiss"
                }
                button {
                    class: "px-3 py-1.5 text-xs text-emerald-300/90 hover:text-emerald-200 border border-emerald-500/30 hover:border-emerald-500/50 rounded",
                    onclick: move |_| on_confirm.call(()),
                    "Confirm"
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Statement CSV import (Phase 5.5) — CIBC chequing.
// -----------------------------------------------------------------------------

/// Default statement_source label for a CIBC chequing import based on
/// today's calendar year + month. Format `"cibc-chequing-YYYY-MM"`,
/// matching the convention used in the event-validation tests and in
/// the reconciliation review's expected source-tag shape.
fn default_statement_source_label() -> String {
    let today = chrono::Utc::now().date_naive();
    format!(
        "cibc-chequing-{}-{:02}",
        chrono::Datelike::year(&today),
        chrono::Datelike::month(&today)
    )
}

#[component]
fn StatementImportView(on_back: EventHandler<()>) -> Element {
    // No bank-specific default — the public engine ships with zero declared
    // accounts (3.4). The field is free-text; the placeholder shows the shape.
    let mut source_account: Signal<String> = use_signal(String::new);
    let mut statement_source: Signal<String> = use_signal(default_statement_source_label);
    let mut commodity: Signal<String> = use_signal(|| "CAD".to_string());
    let mut status: Signal<Option<String>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut importing: Signal<bool> = use_signal(|| false);

    let on_file_picked = move |evt: Event<FormData>| {
        let files = evt.files();
        let Some(file) = files.into_iter().next() else {
            return;
        };
        let src = source_account.read().clone();
        let label = statement_source.read().clone();
        let comm = commodity.read().clone();
        importing.set(true);
        error.set(None);
        status.set(None);
        spawn(async move {
            let bytes = match file.read_bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    error.set(Some(format!("Couldn't read file: {e}")));
                    importing.set(false);
                    return;
                }
            };
            let csv_text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    error.set(Some(
                        "File isn't valid UTF-8 — re-export the CSV from CIBC online banking."
                            .to_string(),
                    ));
                    importing.set(false);
                    return;
                }
            };
            match bridge::invoke_import_cibc_chequing_csv(
                &csv_text,
                &src,
                &label,
                Some(&comm),
            )
            .await
            {
                Ok(result) => status.set(Some(format!(
                    "Imported {} transactions. Each lands in Unmatched, ready for reconciliation review.",
                    result.imported
                ))),
                Err(e) => error.set(Some(format!("Import failed: {e}"))),
            }
            importing.set(false);
        });
    };

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Import statement"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        div { class: "mb-4 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
            div {
                label { class: "block text-xs text-obsidian-text-muted mb-1",
                    "Source account"
                }
                input {
                    class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none",
                    r#type: "text",
                    placeholder: "Assets:Bank:Chequing",
                    value: "{source_account.read()}",
                    oninput: move |e| source_account.set(e.value()),
                }
            }
            div { class: "grid grid-cols-2 gap-3",
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1",
                        "Statement label"
                    }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                        r#type: "text",
                        value: "{statement_source.read()}",
                        oninput: move |e| statement_source.set(e.value()),
                    }
                }
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1",
                        "Commodity"
                    }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                        r#type: "text",
                        value: "{commodity.read()}",
                        oninput: move |e| commodity.set(e.value()),
                    }
                }
            }

            div {
                label { class: "block text-xs text-obsidian-text-muted mb-1",
                    "CSV file"
                }
                input {
                    class: "w-full text-sm text-obsidian-text file:mr-3 file:px-3 file:py-1.5 file:bg-obsidian-accent/90 file:hover:bg-obsidian-accent file:text-black file:rounded file:border-none file:text-xs file:font-semibold disabled:opacity-50",
                    r#type: "file",
                    accept: ".csv,text/csv",
                    disabled: *importing.read(),
                    onchange: on_file_picked,
                }
            }
        }

        if let Some(msg) = error.read().clone() {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        }

        if let Some(msg) = status.read().clone() {
            div { class: "mb-4 p-4 bg-emerald-950/30 border border-emerald-500/30 rounded-lg text-sm text-emerald-200",
                "{msg}"
            }
        }

        div { class: "p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg text-xs text-obsidian-text-muted space-y-2",
            div { class: "font-semibold text-obsidian-text", "What this does" }
            div {
                "Each parsed row becomes one transaction with a posting on the source account and a balancing entry on the Unmatched clearing account. Use the Reconcile screen to pair these against captured receipts or auto-imported transactions."
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Reconciliation review (Phase 5.7) — two-column pairs, merge or dismiss.
// -----------------------------------------------------------------------------

/// Label for the confidence indicator on a candidate row.
fn confidence_label(score: f64) -> &'static str {
    if score >= 0.85 {
        "High"
    } else if score >= 0.6 {
        "Medium"
    } else {
        "Low"
    }
}

/// Tailwind background class for the confidence pill, matching `confidence_label`.
fn confidence_color_class(score: f64) -> &'static str {
    if score >= 0.85 {
        "bg-emerald-500/20 text-emerald-200 border-emerald-500/40"
    } else if score >= 0.6 {
        "bg-amber-400/15 text-amber-200 border-amber-400/30"
    } else {
        "bg-obsidian-text-muted/10 text-obsidian-text-muted border-white/10"
    }
}

#[component]
fn ReconciliationReviewView(on_back: EventHandler<()>) -> Element {
    let mut candidates: Signal<Vec<MatchCandidateView>> = use_signal(Vec::new);
    let mut no_match_rows: Signal<Vec<ReconciliationTxnPreview>> = use_signal(Vec::new);
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    // Dismissed pairs (by "primary|secondary" key) — local-only, not
    // persisted. A "skip for now" affordance that doesn't pollute the
    // event log. Reload re-surfaces them.
    let mut dismissed: Signal<std::collections::HashSet<String>> =
        use_signal(std::collections::HashSet::new);
    // Pair currently being merged — disables the row's buttons so a
    // double-click can't fire two merges.
    let mut merging_pair: Signal<Option<String>> = use_signal(|| None);

    let load_candidates = move || {
        spawn(async move {
            loading.set(true);
            load_error.set(None);
            match bridge::invoke_list_match_candidates(Some(7)).await {
                Ok(rows) => candidates.set(rows),
                Err(e) => load_error.set(Some(e)),
            }
            // No-match rows load is best-effort — if it fails, the
            // matched pairs section still renders.
            if let Ok(rows) = bridge::invoke_list_unmatched_without_candidates(Some(7)).await {
                no_match_rows.set(rows);
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_candidates();
    });

    let merge = move |primary_id: String, secondary_id: String| {
        let key = format!("{primary_id}|{secondary_id}");
        spawn(async move {
            merging_pair.set(Some(key.clone()));
            match bridge::invoke_merge_transactions(&primary_id, &secondary_id).await {
                Ok(_) => {
                    // Refetch — the merged pair drops out, and any other
                    // candidates that referenced the absorbed secondary
                    // also drop.
                    load_candidates();
                }
                Err(e) => load_error.set(Some(format!("Merge failed: {e}"))),
            }
            merging_pair.set(None);
        });
    };

    let mut dismiss = move |primary_id: String, secondary_id: String| {
        let key = format!("{primary_id}|{secondary_id}");
        let mut set = dismissed.read().clone();
        set.insert(key);
        dismissed.set(set);
    };

    let resolve = move |txn_id: String, category: String| {
        spawn(async move {
            if let Err(e) = bridge::invoke_resolve_unmatched(&txn_id, &category).await {
                load_error.set(Some(format!("Resolve failed: {e}")));
                return;
            }
            load_candidates();
        });
    };

    let snapshot = candidates.read().clone();
    let no_match_snapshot = no_match_rows.read().clone();
    let dismissed_set = dismissed.read().clone();
    let is_loading = *loading.read();
    let err_msg = load_error.read().clone();
    let active_merge = merging_pair.read().clone();

    let visible: Vec<MatchCandidateView> = snapshot
        .into_iter()
        .filter(|c| !dismissed_set.contains(&format!("{}|{}", c.primary_id, c.secondary_id)))
        .collect();
    let visible_empty = visible.is_empty();
    let no_match_empty = no_match_snapshot.is_empty();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Reconcile"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        div { class: "mb-4 p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg text-xs text-obsidian-text-muted",
            "Pairs of Unmatched-touching transactions whose amounts cancel out. Merge accepts the pair into one transaction (with the statement side automatically cleared); Skip hides the pair until next reload."
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        }

        if is_loading {
            div { class: "p-6 text-center text-obsidian-text-muted text-sm",
                "Loading candidates…"
            }
        } else if visible.is_empty() {
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                "No reconciliation candidates. Import a statement or wait for more auto-imported transactions to accumulate."
            }
        } else if !visible_empty {
            div { class: "space-y-3",
                for c in visible {
                    {
                        let key = format!("{}|{}", c.primary_id, c.secondary_id);
                        let is_merging = active_merge.as_deref() == Some(key.as_str());
                        rsx! {
                            CandidateCard {
                                key: "{key}",
                                cand: c.clone(),
                                is_merging,
                                on_merge: {
                                    let p = c.primary_id.clone();
                                    let s = c.secondary_id.clone();
                                    move |_| merge(p.clone(), s.clone())
                                },
                                on_dismiss: {
                                    let p = c.primary_id.clone();
                                    let s = c.secondary_id.clone();
                                    move |_| dismiss(p.clone(), s.clone())
                                },
                            }
                        }
                    }
                }
            }
        }

        // --- No-match path (5.7) — Unmatched-touching transactions with
        // no candidate. User assigns a category to convert the Unmatched
        // leg into a real category leg; statement-sourced rows auto-clear.
        if !no_match_empty {
            div { class: "mt-6 mb-3 border-b border-white/5 pb-2",
                h2 { class: "text-sm font-bold text-obsidian-text",
                    "No-match transactions ({no_match_snapshot.len()})"
                }
                p { class: "text-xs text-obsidian-text-muted mt-1",
                    "Statement rows or auto-imports with no pairing candidate — assign a category to resolve each."
                }
            }
            div { class: "space-y-3",
                for row in no_match_snapshot {
                    NoMatchRowCard {
                        key: "{row.txn_id}",
                        row: row.clone(),
                        on_resolve: {
                            let id = row.txn_id.clone();
                            move |category: String| resolve(id.clone(), category)
                        },
                    }
                }
            }
        } else if !is_loading && visible_empty {
            // True empty state — no pairs AND no no-match rows.
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                "No reconciliation candidates. Import a statement or wait for more auto-imported transactions to accumulate."
            }
        }
    }
}

#[component]
fn NoMatchRowCard(row: ReconciliationTxnPreview, on_resolve: EventHandler<String>) -> Element {
    let mut category_input: Signal<String> = use_signal(String::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let source_label = row.statement_source.as_deref().unwrap_or("captured");

    let mut submit = move || {
        let cat = category_input.read().trim().to_string();
        if cat.is_empty() {
            error.set(Some("Category required".to_string()));
            return;
        }
        error.set(None);
        on_resolve.call(cat);
    };

    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
            div { class: "min-w-0",
                div { class: "text-sm font-semibold text-obsidian-text truncate",
                    "{row.description}"
                }
                div { class: "text-xs text-obsidian-text-muted mt-1",
                    "{row.date} · {row.unmatched_amount} {row.unmatched_commodity} · {source_label}"
                }
            }
            div { class: "flex gap-2",
                input {
                    class: "flex-1 px-3 py-1.5 bg-obsidian-bg border border-white/10 rounded text-xs text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none",
                    r#type: "text",
                    placeholder: "Expenses:Groceries",
                    value: "{category_input.read()}",
                    oninput: move |e| category_input.set(e.value()),
                }
                button {
                    class: "px-3 py-1.5 text-xs font-semibold text-black bg-obsidian-accent/90 hover:bg-obsidian-accent rounded",
                    onclick: move |_| submit(),
                    "Resolve"
                }
            }
            if let Some(msg) = error.read().clone() {
                div { class: "text-xs text-red-300 px-1", "{msg}" }
            }
        }
    }
}

#[component]
fn CandidateCard(
    cand: MatchCandidateView,
    is_merging: bool,
    on_merge: EventHandler<()>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let conf = confidence_label(cand.score);
    let conf_class = confidence_color_class(cand.score);
    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
            div { class: "flex items-center gap-2",
                span {
                    class: "px-2 py-0.5 text-xs font-semibold border rounded-full {conf_class}",
                    "{conf}"
                }
                span { class: "text-xs text-obsidian-text-muted",
                    "{cand.days_apart} day(s) apart · descriptions {(cand.description_similarity * 100.0) as u32}% similar"
                }
                if cand.clears_statement {
                    span { class: "text-xs text-obsidian-accent",
                        "· clears statement"
                    }
                }
            }
            div { class: "grid grid-cols-2 gap-3",
                CandidateSide { txn: cand.primary.clone() }
                CandidateSide { txn: cand.secondary.clone() }
            }
            div { class: "flex gap-2 justify-end pt-1",
                button {
                    class: "px-3 py-1.5 text-xs text-obsidian-text-muted hover:text-obsidian-text border border-white/10 hover:border-white/20 rounded disabled:opacity-50",
                    disabled: is_merging,
                    onclick: move |_| on_dismiss.call(()),
                    "Skip"
                }
                button {
                    class: "px-4 py-1.5 text-xs font-semibold text-black bg-obsidian-accent/90 hover:bg-obsidian-accent rounded disabled:opacity-50",
                    disabled: is_merging,
                    onclick: move |_| on_merge.call(()),
                    if is_merging { "Merging…" } else { "Merge" }
                }
            }
        }
    }
}

#[component]
fn CandidateSide(txn: crate::types::ReconciliationTxnPreview) -> Element {
    let source_label = txn.statement_source.as_deref().unwrap_or("captured");
    rsx! {
        div { class: "p-3 bg-obsidian-bg/60 border border-white/5 rounded text-xs space-y-1",
            div { class: "text-obsidian-text font-medium truncate",
                "{txn.description}"
            }
            div { class: "text-obsidian-text-muted",
                "{txn.date} · {txn.unmatched_amount} {txn.unmatched_commodity}"
            }
            div { class: "text-obsidian-text-muted/80 italic truncate",
                "{source_label}"
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Balance check (Phase 5.8) — cleared vs statement closing balance.
// -----------------------------------------------------------------------------

#[component]
fn BalanceCheckFormView(on_back: EventHandler<()>) -> Element {
    // No bank-specific default (3.4) — placeholder shows the shape instead.
    let mut account: Signal<String> = use_signal(String::new);
    let mut commodity: Signal<String> = use_signal(|| "CAD".to_string());
    let mut statement_balance: Signal<String> = use_signal(String::new);
    let mut as_of: Signal<String> = use_signal(|| chrono::Utc::now().date_naive().to_string());
    let mut result: Signal<Option<BalanceCheckView>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut checking: Signal<bool> = use_signal(|| false);

    let mut run_check = move || {
        let acc = account.read().clone();
        let comm = commodity.read().clone();
        let bal = statement_balance.read().clone();
        let asof = as_of.read().clone();
        if bal.trim().is_empty() {
            error.set(Some("Statement balance is required".to_string()));
            return;
        }
        checking.set(true);
        error.set(None);
        spawn(async move {
            match bridge::invoke_check_account_balance(&acc, &comm, &bal, Some(&asof)).await {
                Ok(r) => result.set(Some(r)),
                Err(e) => error.set(Some(format!("Check failed: {e}"))),
            }
            checking.set(false);
        });
    };

    let snapshot = result.read().clone();
    let err_msg = error.read().clone();
    let is_checking = *checking.read();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Balance check"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        div { class: "mb-4 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
            div {
                label { class: "block text-xs text-obsidian-text-muted mb-1", "Account" }
                input {
                    class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none",
                    r#type: "text",
                    placeholder: "Assets:Bank:Chequing",
                    value: "{account.read()}",
                    oninput: move |e| account.set(e.value()),
                }
            }
            div { class: "grid grid-cols-3 gap-3",
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1", "Commodity" }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                        r#type: "text",
                        value: "{commodity.read()}",
                        oninput: move |e| commodity.set(e.value()),
                    }
                }
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1", "Statement balance" }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                        r#type: "text",
                        inputmode: "decimal",
                        placeholder: "1500.00",
                        value: "{statement_balance.read()}",
                        oninput: move |e| statement_balance.set(e.value()),
                    }
                }
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1", "As of" }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                        r#type: "date",
                        value: "{as_of.read()}",
                        oninput: move |e| as_of.set(e.value()),
                    }
                }
            }
            div { class: "flex justify-end",
                button {
                    class: "px-4 py-2 bg-obsidian-accent/90 hover:bg-obsidian-accent text-black text-sm font-semibold rounded disabled:opacity-50",
                    disabled: is_checking,
                    onclick: move |_| run_check(),
                    if is_checking { "Checking…" } else { "Check" }
                }
            }
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        }

        if let Some(r) = snapshot {
            BalanceCheckResultCard { result: r }
        }
    }
}

#[component]
fn BalanceCheckResultCard(result: BalanceCheckView) -> Element {
    let verdict_class = if result.ok {
        "bg-emerald-950/30 border-emerald-500/30 text-emerald-200"
    } else {
        "bg-amber-950/30 border-amber-500/30 text-amber-200"
    };
    let verdict_label = if result.ok {
        "Balanced — cleared total matches the statement.".to_string()
    } else {
        format!(
            "Discrepancy: {} {} ({} cleared vs {} on statement)",
            result.discrepancy, result.commodity, result.cleared_total, result.statement_balance
        )
    };
    rsx! {
        div { class: "p-4 border rounded-lg space-y-2 {verdict_class}",
            div { class: "text-sm font-semibold", "{verdict_label}" }
            div { class: "text-xs opacity-80",
                "Account: {result.account} · cleared total {result.cleared_total} {result.commodity}"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 6.2 + 6.3 — hledger journal import view.
//
// Three states: Idle (path input + Preview button), Previewed (per-account
// table + drop/rename + Commit), Done (result summary). One signal per state
// keeps the UI flat — no enum nesting needed since transitions are linear
// (Idle → Previewed → Done) and a Back button covers the only reverse path.
// ---------------------------------------------------------------------------

#[component]
fn JournalImportView(on_back: EventHandler<()>) -> Element {
    let mut path: Signal<String> = use_signal(String::new);
    let mut preview: Signal<Option<JournalImportPreview>> = use_signal(|| None);
    let mut result: Signal<Option<JournalImportResult>> = use_signal(|| None);
    let mut loading: Signal<bool> = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut apply_a2: Signal<bool> = use_signal(|| true);
    // Set of account names the user has unchecked. Default: include all.
    let mut dropped: Signal<std::collections::HashSet<String>> =
        use_signal(std::collections::HashSet::new);
    // Per-account rename mapping. Empty string means no rename. Keyed by
    // original account name.
    let mut renames: Signal<std::collections::HashMap<String, String>> =
        use_signal(std::collections::HashMap::new);

    let on_preview = move |_| {
        let p = path.read().trim().to_string();
        if p.is_empty() {
            error.set(Some("Enter a path to your main.ledger file.".into()));
            return;
        }
        loading.set(true);
        error.set(None);
        preview.set(None);
        result.set(None);
        dropped.set(std::collections::HashSet::new());
        renames.set(std::collections::HashMap::new());
        spawn(async move {
            match bridge::invoke_preview_journal_import(&p).await {
                Ok(view) => preview.set(Some(view)),
                Err(e) => error.set(Some(format!("Preview failed: {e}"))),
            }
            loading.set(false);
        });
    };

    let on_commit = move |_| {
        let p = path.read().trim().to_string();
        let drops: Vec<String> = dropped.read().iter().cloned().collect();
        let rename_map: std::collections::HashMap<String, String> = renames
            .read()
            .iter()
            .filter_map(|(k, v)| {
                let trimmed = v.trim();
                if trimmed.is_empty() || trimmed == k {
                    None
                } else {
                    Some((k.clone(), trimmed.to_string()))
                }
            })
            .collect();
        let plan = JournalImportPlan {
            accounts_to_drop: drops,
            account_renames: rename_map,
            apply_a2_rewriter: *apply_a2.read(),
        };
        loading.set(true);
        error.set(None);
        spawn(async move {
            match bridge::invoke_commit_journal_import(&p, plan).await {
                Ok(res) => {
                    result.set(Some(res));
                    preview.set(None);
                }
                Err(e) => error.set(Some(format!("Commit failed: {e}"))),
            }
            loading.set(false);
        });
    };

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Import journal"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        if let Some(err) = error.read().clone() {
            div { class: "mb-4 p-3 bg-rose-500/10 border border-rose-500/30 rounded-lg text-sm text-rose-300",
                "{err}"
            }
        }

        // --- Idle / path-entry state ---
        if preview.read().is_none() && result.read().is_none() {
            div { class: "mb-4 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
                div {
                    label { class: "block text-xs text-obsidian-text-muted mb-1",
                        "Path to main.ledger"
                    }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                        r#type: "text",
                        placeholder: "/home/you/journals/main.ledger",
                        value: "{path.read()}",
                        oninput: move |e| path.set(e.value()),
                    }
                    p { class: "text-xs text-obsidian-text-muted mt-1",
                        "Parses the file plus any included sub-journals. Per-file errors are surfaced for review — they never abort the walk."
                    }
                }
                button {
                    class: "px-4 py-2 bg-obsidian-accent/90 hover:bg-obsidian-accent text-black rounded text-sm font-semibold disabled:opacity-50",
                    disabled: *loading.read(),
                    onclick: on_preview,
                    if *loading.read() { "Previewing…" } else { "Preview" }
                }
            }
        }

        // --- Previewed state ---
        if let Some(view) = preview.read().clone() {
            div { class: "mb-4 p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
                div { class: "grid grid-cols-2 md:grid-cols-4 gap-3 text-sm",
                    StatCell { label: "Files", value: "{view.files_parsed}" }
                    StatCell { label: "Transactions", value: "{view.transactions_count}" }
                    StatCell { label: "Accounts", value: "{view.per_account.len()}" }
                    StatCell { label: "Commodities", value: "{view.commodities.len()}" }
                }
                div { class: "text-xs text-obsidian-text-muted",
                    "Parsed root: {view.root}"
                }
                if view.already_imported_count > 0 {
                    div { class: "text-xs text-amber-300",
                        "{view.already_imported_count} transactions already in your projection — they'll be skipped on commit."
                    }
                }

                label { class: "flex items-center gap-2 text-sm text-obsidian-text",
                    input {
                        r#type: "checkbox",
                        checked: *apply_a2.read(),
                        onchange: move |e| apply_a2.set(e.value() == "true"),
                    }
                    span { "Apply A2 business→tag rewrite (recommended)" }
                }
                p { class: "text-xs text-obsidian-text-muted -mt-1",
                    "Rewrites Expenses:Business:* into the plain category with a type:business posting tag."
                }
            }

            if !view.parse_errors.is_empty() {
                div { class: "mb-4 p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-xs text-amber-200 space-y-1",
                    div { class: "font-semibold mb-1",
                        "{view.parse_errors.len()} files had issues (continuing without them):"
                    }
                    for err in view.parse_errors.iter().take(10) {
                        div { class: "truncate", "• {err.path}: {err.message}" }
                    }
                    if view.parse_errors.len() > 10 {
                        div { "… and {view.parse_errors.len() - 10} more" }
                    }
                }
            }

            if !view.balance_failures.is_empty() {
                div { class: "mb-4 p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-xs text-amber-200 space-y-1",
                    div { class: "font-semibold mb-1",
                        "{view.balance_failures.len()} transactions couldn't be balanced:"
                    }
                    for fail in view.balance_failures.iter().take(10) {
                        div { class: "truncate", "• {fail}" }
                    }
                }
            }

            div { class: "mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text mb-2", "Accounts" }
                p { class: "text-xs text-obsidian-text-muted mb-3",
                    "Uncheck any account you don't want to bring over. Renames take effect on commit."
                }
                div { class: "space-y-2",
                    for stats in view.per_account.iter().cloned() {
                        AccountRow {
                            stats: stats.clone(),
                            dropped: dropped,
                            renames: renames,
                        }
                    }
                }
            }

            if !view.sample_transactions.is_empty() {
                div { class: "mb-4",
                    h2 { class: "text-lg font-bold text-obsidian-text mb-2",
                        "First {view.sample_transactions.len()} transactions"
                    }
                    div { class: "space-y-2 max-h-96 overflow-y-auto pr-1",
                        for txn in view.sample_transactions.iter().cloned() {
                            SampleTxnRow { txn: txn.clone() }
                        }
                    }
                }
            }

            div { class: "flex gap-2",
                button {
                    class: "px-4 py-2 bg-obsidian-accent/90 hover:bg-obsidian-accent text-black rounded text-sm font-semibold disabled:opacity-50",
                    disabled: *loading.read(),
                    onclick: on_commit,
                    if *loading.read() { "Committing…" } else { "Commit import" }
                }
                button {
                    class: "px-4 py-2 bg-obsidian-sidebar border border-white/10 hover:border-white/20 text-obsidian-text rounded text-sm",
                    onclick: move |_| { preview.set(None); error.set(None); },
                    "Cancel"
                }
            }
        }

        // --- Done state ---
        if let Some(res) = result.read().clone() {
            div { class: "mb-4 p-4 bg-emerald-500/10 border border-emerald-500/30 rounded-lg text-sm text-emerald-200 space-y-2",
                div { class: "font-semibold",
                    "Imported {res.committed_count} transactions"
                }
                if res.skipped_existing_count > 0 {
                    div { "Skipped {res.skipped_existing_count} already-imported transactions (idempotent)." }
                }
                if res.dropped_count > 0 {
                    div { "Dropped {res.dropped_count} transactions touching unchecked accounts." }
                }
                if res.a2_rewrites > 0 {
                    div { "Rewrote {res.a2_rewrites} business-account postings to type:business tags." }
                }
                if !res.balance_failures.is_empty() {
                    div { class: "text-amber-300",
                        "{res.balance_failures.len()} transactions couldn't be balanced and were skipped."
                    }
                }
            }
            button {
                class: "px-4 py-2 bg-obsidian-sidebar border border-white/10 hover:border-white/20 text-obsidian-text rounded text-sm",
                onclick: move |_| { result.set(None); on_back.call(()); },
                "Back to Finances"
            }
        }
    }
}

#[component]
fn StatCell(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "p-2 bg-obsidian-bg border border-white/5 rounded",
            div { class: "text-xs text-obsidian-text-muted", "{label}" }
            div { class: "text-lg font-semibold text-obsidian-text", "{value}" }
        }
    }
}

#[component]
fn AccountRow(
    stats: crate::types::JournalImportAccountStats,
    dropped: Signal<std::collections::HashSet<String>>,
    renames: Signal<std::collections::HashMap<String, String>>,
) -> Element {
    let account_for_drop = stats.account.clone();
    let account_for_rename_key = stats.account.clone();
    let account_for_rename_value = stats.account.clone();
    let account_for_classes = stats.account.clone();
    let account_for_default = stats.account.clone();
    let included = !dropped.read().contains(&stats.account);
    let current_rename = renames.read().get(&stats.account).cloned();
    let rename_value = current_rename.unwrap_or_else(|| account_for_default.clone());
    let row_class = if included {
        "flex items-center gap-2 p-2 bg-obsidian-bg/60 border border-white/5 rounded"
    } else {
        "flex items-center gap-2 p-2 bg-obsidian-bg/30 border border-white/5 rounded opacity-50"
    };
    rsx! {
        div { class: "{row_class}",
            input {
                r#type: "checkbox",
                checked: included,
                onchange: move |e| {
                    let mut set = dropped.write();
                    if e.value() == "true" {
                        set.remove(&account_for_drop);
                    } else {
                        set.insert(account_for_drop.clone());
                    }
                },
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-sm text-obsidian-text font-mono truncate",
                    "{account_for_classes}"
                }
                div { class: "text-xs text-obsidian-text-muted",
                    "{stats.transaction_count} txn · {stats.posting_count} postings"
                }
            }
            input {
                class: "w-44 px-2 py-1 bg-obsidian-bg border border-white/10 rounded text-xs text-obsidian-text focus:border-obsidian-accent/60 focus:outline-none",
                r#type: "text",
                placeholder: "rename to…",
                value: "{rename_value}",
                oninput: move |e| {
                    let new_value = e.value();
                    let mut map = renames.write();
                    if new_value.trim().is_empty() || new_value.trim() == account_for_rename_key {
                        map.remove(&account_for_rename_key);
                    } else {
                        map.insert(account_for_rename_value.clone(), new_value);
                    }
                },
            }
        }
    }
}

#[component]
fn SampleTxnRow(txn: crate::types::JournalImportSampleTxn) -> Element {
    rsx! {
        div { class: "p-2 bg-obsidian-bg/60 border border-white/5 rounded text-xs",
            div { class: "flex justify-between items-baseline",
                div { class: "font-semibold text-obsidian-text", "{txn.date} · {txn.description}" }
                div { class: "text-obsidian-text-muted font-mono", "{txn.txn_id}" }
            }
            div { class: "mt-1 space-y-0.5 font-mono",
                for posting in txn.postings.iter() {
                    div { class: "flex justify-between text-obsidian-text-muted",
                        span { class: "truncate", "{posting.account}" }
                        span {
                            "{posting.amount} {posting.commodity}"
                            if let Some(rate) = posting.fx_rate.as_ref() {
                                if let Some(quote) = posting.fx_quote.as_ref() {
                                    span { class: "ml-1 text-emerald-400",
                                        " @ {rate} {quote}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_share_mime_routes_known_image_subtypes() {
        assert_eq!(classify_share_mime("image/jpeg", "r.jpg"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/png", "r.png"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/heic", "r.heic"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/heif", "r.heif"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/webp", "r.webp"), Some(DocumentKind::Photo));
    }

    #[test]
    fn classify_share_mime_is_case_insensitive() {
        assert_eq!(classify_share_mime("Image/JPEG", "r.jpg"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("APPLICATION/PDF", "s"), Some(DocumentKind::Pdf));
    }

    #[test]
    fn classify_share_mime_accepts_well_declared_pdf_regardless_of_filename() {
        // application/pdf wins even when filename has no .pdf extension —
        // covers the renamed-PDF case.
        assert_eq!(classify_share_mime("application/pdf", "statement"), Some(DocumentKind::Pdf));
    }

    #[test]
    fn classify_share_mime_rescues_stripped_mime_via_filename() {
        // The canonical Android share-target case: ContentProvider couldn't
        // sniff the MIME, MainActivity.kt fell back to octet-stream, but the
        // filename extension is still intact.
        assert_eq!(
            classify_share_mime("application/octet-stream", "chequing-march.pdf"),
            Some(DocumentKind::Pdf),
        );
    }

    #[test]
    fn classify_share_mime_trusts_legacy_pdf_mime_with_filename() {
        // application/x-pdf is a legacy non-standard PDF MIME; permissiveness
        // here is intentional (see doc-comment on classify_share_mime).
        assert_eq!(
            classify_share_mime("application/x-pdf", "s.pdf"),
            Some(DocumentKind::Pdf),
        );
    }

    #[test]
    fn classify_share_mime_rejects_text_family_even_with_pdf_filename() {
        // text/* shares belong in EmailCapture — refusing here drops the
        // share gracefully and lets the user pick the right flow manually.
        assert_eq!(classify_share_mime("text/html", "r.pdf"), None);
        assert_eq!(classify_share_mime("text/plain", "anything"), None);
    }

    #[test]
    fn classify_share_mime_rejects_unknown_image_subtype() {
        // Image allowlist is intentional — image/tiff or image/svg+xml
        // aren't realistic receipt formats and Gemini's photo extraction
        // is calibrated for the listed subtypes.
        assert_eq!(classify_share_mime("image/tiff", "r.tiff"), None);
        assert_eq!(classify_share_mime("image/svg+xml", "r.svg"), None);
    }

    // --- Phase 4.2 attachment-render classifier --------------------------

    #[test]
    fn classify_attachment_routes_images_to_image_render() {
        assert_eq!(classify_attachment("image/jpeg"), AttachmentRender::Image);
        assert_eq!(classify_attachment("image/png"), AttachmentRender::Image);
        assert_eq!(classify_attachment("IMAGE/HEIC"), AttachmentRender::Image);
    }

    #[test]
    fn classify_attachment_routes_pdf_variants_to_pdf_render() {
        assert_eq!(classify_attachment("application/pdf"), AttachmentRender::Pdf);
        assert_eq!(classify_attachment("APPLICATION/PDF"), AttachmentRender::Pdf);
        assert_eq!(classify_attachment("application/x-pdf"), AttachmentRender::Pdf);
    }

    #[test]
    fn classify_attachment_falls_back_to_other_for_unknown_mime() {
        assert_eq!(classify_attachment("text/plain"), AttachmentRender::Other);
        assert_eq!(classify_attachment("application/zip"), AttachmentRender::Other);
        assert_eq!(classify_attachment(""), AttachmentRender::Other);
    }

    #[test]
    fn extract_attachment_meta_decodes_complete_ref() {
        let v = serde_json::json!({
            "sha256": "abc123",
            "filename": "receipt.jpg",
            "mime_type": "image/jpeg",
            "size": 1024,
        });
        let meta = extract_attachment_meta(&v).unwrap();
        assert_eq!(meta.sha256, "abc123");
        assert_eq!(meta.filename, "receipt.jpg");
        assert_eq!(meta.mime_type, "image/jpeg");
        assert_eq!(meta.size, 1024);
    }

    #[test]
    fn extract_attachment_meta_returns_none_when_required_field_missing() {
        let v = serde_json::json!({
            "filename": "x.pdf",
            "mime_type": "application/pdf",
            "size": 5000,
        });
        assert!(extract_attachment_meta(&v).is_none(), "missing sha256 should fail");
    }

    // --- Phase 4.4 money formatter --------------------------------------

    #[test]
    fn format_money_pads_fractional_to_two_digits() {
        assert_eq!(format_money("5", "CAD"), "5.00 CAD");
        assert_eq!(format_money("5.4", "CAD"), "5.40 CAD");
        assert_eq!(format_money("5.40", "CAD"), "5.40 CAD");
    }

    #[test]
    fn format_money_preserves_extra_precision_for_crypto() {
        // BTC-style amounts shouldn't lose precision to a 2-digit floor.
        assert_eq!(format_money("0.00123456", "BTC"), "0.00123456 BTC");
    }

    #[test]
    fn format_money_adds_thousands_separators() {
        assert_eq!(format_money("1234.5", "CAD"), "1,234.50 CAD");
        assert_eq!(format_money("1234567.89", "CAD"), "1,234,567.89 CAD");
        assert_eq!(format_money("999.99", "CAD"), "999.99 CAD");
    }

    #[test]
    fn format_money_handles_negative_amounts() {
        assert_eq!(format_money("-1450.18", "CAD"), "-1,450.18 CAD");
        assert_eq!(format_money("-1.5", "CAD"), "-1.50 CAD");
    }

    // --- Phase 4.5+4.6 dashboard helpers ---------------------------------

    fn bucket(month: &str, income: &str, spending: &str) -> MonthlyTrendBucketView {
        MonthlyTrendBucketView {
            month: month.into(),
            income: income.into(),
            spending: spending.into(),
        }
    }

    #[test]
    fn max_trend_magnitude_picks_largest_absolute_across_income_and_spending() {
        let b = vec![
            bucket("2026-01", "3200.00", "2940.18"),
            bucket("2026-02", "3450.00", "1500.00"),
            bucket("2026-03", "1820.00", "1450.18"),
        ];
        assert_eq!(max_trend_magnitude(&b), Some(3450.0));
    }

    #[test]
    fn max_trend_magnitude_returns_none_when_all_zero() {
        let b = vec![bucket("2026-01", "0", "0"), bucket("2026-02", "0", "0")];
        assert_eq!(max_trend_magnitude(&b), None);
    }

    #[test]
    fn bar_height_pct_scales_correctly() {
        assert_eq!(bar_height_pct("1000", 4000.0), 25.0);
        assert_eq!(bar_height_pct("4000", 4000.0), 100.0);
        // Negative amounts use absolute value (income amounts are always
        // positive in the trend, but defensive against a negative slipping in).
        assert_eq!(bar_height_pct("-500", 4000.0), 12.5);
    }

    #[test]
    fn bar_height_pct_clamps_to_100() {
        assert_eq!(bar_height_pct("9999", 1000.0), 100.0);
    }

    #[test]
    fn bar_height_pct_zero_scale_returns_zero() {
        assert_eq!(bar_height_pct("100", 0.0), 0.0);
    }

    #[test]
    fn cadence_label_names_common_cadences() {
        assert_eq!(cadence_label(7), "weekly");
        assert_eq!(cadence_label(14), "biweekly");
        assert_eq!(cadence_label(30), "monthly");
        assert_eq!(cadence_label(31), "monthly");
    }

    #[test]
    fn cadence_label_falls_back_to_every_n_days() {
        assert_eq!(cadence_label(3), "every 3 days");
        assert_eq!(cadence_label(60), "every 60 days");
    }

    // --- Budget helpers (5.1) ---

    #[test]
    fn period_label_uses_named_cadence() {
        assert_eq!(period_label("monthly"), "Monthly");
        assert_eq!(period_label("biweekly"), "Biweekly");
        assert_eq!(period_label("weekly"), "Weekly");
    }

    #[test]
    fn period_label_falls_back_to_raw_for_unknown() {
        // A hand-crafted custom:N round-trips through the picker unchanged.
        assert_eq!(period_label("custom:90"), "custom:90");
    }

    #[test]
    fn normalize_category_trims_and_rejects_empty() {
        assert_eq!(normalize_category("  Expenses:Groceries "), Some("Expenses:Groceries".to_string()));
        assert_eq!(normalize_category(""), None);
        assert_eq!(normalize_category("   "), None);
    }

    #[test]
    fn validate_amount_accepts_positive_decimal() {
        assert_eq!(validate_amount(" 12.50 "), Ok("12.50".to_string()));
        assert_eq!(validate_amount("0.01"), Ok("0.01".to_string()));
    }

    #[test]
    fn validate_amount_rejects_blank_or_zero_or_negative() {
        assert!(validate_amount("").is_err());
        assert!(validate_amount("   ").is_err());
        assert!(validate_amount("0").is_err());
        assert!(validate_amount("-5").is_err());
    }

    #[test]
    fn validate_amount_rejects_non_numeric() {
        assert!(validate_amount("abc").is_err());
        assert!(validate_amount("12.5x").is_err());
    }

    // --- Budget progress helpers (5.2) ---

    #[test]
    fn progress_color_picks_red_when_over_budget() {
        assert_eq!(progress_color_class(105.0, true), "bg-red-500/70");
    }

    #[test]
    fn progress_color_picks_amber_in_warning_band() {
        assert_eq!(progress_color_class(75.0, false), "bg-amber-400/70");
        assert_eq!(progress_color_class(99.9, false), "bg-amber-400/70");
    }

    #[test]
    fn progress_color_picks_green_under_warning_band() {
        assert_eq!(progress_color_class(0.0, false), "bg-emerald-500/70");
        assert_eq!(progress_color_class(50.0, false), "bg-emerald-500/70");
    }

    #[test]
    fn progress_width_pct_clamps_at_100() {
        // Over-budget bars stop filling visually at 100% — the verdict text
        // surfaces the true over-by-X% number below.
        assert_eq!(progress_width_pct(150.0), 100.0);
        assert_eq!(progress_width_pct(50.0), 50.0);
        assert_eq!(progress_width_pct(-10.0), 0.0);
    }

    #[test]
    fn period_range_label_joins_endpoints() {
        assert_eq!(
            period_range_label("2026-05-01", "2026-05-31"),
            "2026-05-01 – 2026-05-31"
        );
    }

    // --- Recurring helpers (5.4) ---

    #[test]
    fn recurring_cadence_label_names_known_cadences() {
        assert_eq!(recurring_cadence_label(7), "weekly");
        assert_eq!(recurring_cadence_label(14), "biweekly");
        assert_eq!(recurring_cadence_label(30), "monthly");
        assert_eq!(recurring_cadence_label(31), "monthly");
    }

    #[test]
    fn recurring_cadence_label_falls_back_to_every_n_days() {
        assert_eq!(recurring_cadence_label(90), "every 90 days");
    }

    // --- Reconciliation helpers (5.7) ---

    #[test]
    fn confidence_label_thresholds() {
        assert_eq!(confidence_label(0.95), "High");
        assert_eq!(confidence_label(0.85), "High");
        assert_eq!(confidence_label(0.7), "Medium");
        assert_eq!(confidence_label(0.6), "Medium");
        assert_eq!(confidence_label(0.4), "Low");
    }

    #[test]
    fn confidence_color_class_aligns_with_label() {
        assert!(confidence_color_class(0.95).contains("emerald"));
        assert!(confidence_color_class(0.7).contains("amber"));
        assert!(confidence_color_class(0.4).contains("muted"));
    }
}
