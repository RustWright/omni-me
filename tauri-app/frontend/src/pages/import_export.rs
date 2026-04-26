//! Obsidian import/export UI — lives inside the Settings page as a
//! self-contained section. Own state machine (Idle → Scanning → Previewing
//! → Committing → Done) keeps the flow scoped to this one component.

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::bridge;
use crate::types::{
    AcceptedImportRow, ImportCommitSummary, ImportPreviewRow, ImportPreviewSummary,
};

/// Distinct phases the user moves through during import. Export is a
/// separate, simpler flow — just a path input + button + summary.
#[derive(Clone, PartialEq)]
enum ImportPhase {
    Idle,
    Scanning,
    Previewing(ImportPreviewSummary),
    Committing,
    Done(ImportCommitSummary),
    Error(String),
}

#[derive(Clone, PartialEq)]
enum ExportPhase {
    Idle,
    Exporting,
    Done { written: usize, errors: usize },
    Error(String),
}

#[component]
pub fn ImportExportSection() -> Element {
    rsx! {
        div { class: "mb-10 space-y-6",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "Obsidian Import / Export" }
                p { class: "text-xs text-obsidian-text-muted mt-1",
                    "Bring an existing vault in, or write your current data out as a fresh vault."
                }
            }

            ImportFlow {}
            ExportFlow {}
        }
    }
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[component]
fn ImportFlow() -> Element {
    let mut root = use_signal(String::new);
    let mut phase = use_signal(|| ImportPhase::Idle);

    // Per-row decisions. Keyed on `row.path`; presence in `skipped` means
    // the user unchecked it. `overrides` is for edited titles/dates.
    let mut skipped = use_signal(HashSet::<String>::new);
    let mut overrides = use_signal(std::collections::HashMap::<String, String>::new);

    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg space-y-3",
            h3 { class: "text-sm font-bold text-obsidian-text", "Import from Obsidian" }

            // Step 1: Path input + scan
            if matches!(*phase.read(), ImportPhase::Idle | ImportPhase::Error(_)) {
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-3 py-2 bg-obsidian-bg border border-white/10 rounded-md text-sm text-obsidian-text font-mono placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent",
                        r#type: "text",
                        placeholder: "/path/to/obsidian/vault",
                        value: "{root}",
                        oninput: move |e| root.set(e.value()),
                    }
                    button {
                        class: "px-4 py-2 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-40",
                        disabled: root.read().trim().is_empty(),
                        onclick: move |_| {
                            let r = root.read().trim().to_string();
                            if r.is_empty() { return; }
                            phase.set(ImportPhase::Scanning);
                            skipped.set(HashSet::new());
                            overrides.set(std::collections::HashMap::new());
                            spawn(async move {
                                match bridge::invoke_preview_import(&r).await {
                                    Ok(summary) => phase.set(ImportPhase::Previewing(summary)),
                                    Err(e) => phase.set(ImportPhase::Error(e)),
                                }
                            });
                        },
                        "Scan Vault"
                    }
                }

                if let ImportPhase::Error(err) = &*phase.read() {
                    div { class: "p-2 bg-red-900/20 text-red-400 border border-red-900/50 rounded text-xs",
                        "{err}"
                    }
                }
            }

            // Step 2: Scanning spinner
            if matches!(*phase.read(), ImportPhase::Scanning) {
                div { class: "py-6 text-center text-obsidian-text-muted text-sm",
                    "Scanning vault..."
                }
            }

            // Step 3: Preview + per-row decisions
            {
                let current = phase.read().clone();
                if let ImportPhase::Previewing(summary) = current {
                    let summary_for_commit = summary.clone();
                    let total = summary.rows.iter().filter(|r| r.kind != "error").count();
                    let accepted = total.saturating_sub(skipped.read().len());
                    rsx! {
                        ImportPreview {
                            summary: summary.clone(),
                            skipped: skipped,
                            overrides: overrides,
                        }

                        div { class: "flex justify-between items-center pt-3 border-t border-white/5",
                            div { class: "text-xs text-obsidian-text-muted",
                                "{accepted} of {total} notes will be imported."
                            }
                            div { class: "flex gap-2",
                                button {
                                    class: "px-3 py-1.5 text-xs font-semibold rounded-md bg-obsidian-sidebar border border-white/10 text-obsidian-text hover:bg-white/5",
                                    onclick: move |_| phase.set(ImportPhase::Idle),
                                    "Cancel"
                                }
                                button {
                                    class: "px-4 py-1.5 text-sm font-bold rounded-md bg-obsidian-accent text-white hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                                    disabled: accepted == 0,
                                    onclick: move |_| {
                                        let summary = summary_for_commit.clone();
                                        let skipped_snapshot = skipped.read().clone();
                                        let overrides_snapshot = overrides.read().clone();
                                        phase.set(ImportPhase::Committing);
                                        spawn(async move {
                                            let rows: Vec<AcceptedImportRow> = summary
                                                .rows
                                                .into_iter()
                                                .filter(|r| r.kind != "error" && !skipped_snapshot.contains(&r.path))
                                                .map(|r| AcceptedImportRow {
                                                    override_key: overrides_snapshot.get(&r.path).cloned(),
                                                    path: r.path,
                                                    kind: r.kind,
                                                })
                                                .collect();
                                            match bridge::invoke_commit_import(rows).await {
                                                Ok(s) => phase.set(ImportPhase::Done(s)),
                                                Err(e) => phase.set(ImportPhase::Error(e)),
                                            }
                                        });
                                    },
                                    "Commit Import"
                                }
                            }
                        }
                    }
                } else { rsx! {} }
            }

            // Step 4: Committing spinner
            if matches!(*phase.read(), ImportPhase::Committing) {
                div { class: "py-4 text-center text-obsidian-text-muted text-sm",
                    "Writing events..."
                }
            }

            // Step 5: Done
            {
                let current = phase.read().clone();
                if let ImportPhase::Done(summary) = current {
                    let has_errors = !summary.errors.is_empty();
                    rsx! {
                        div { class: "p-3 bg-obsidian-accent/10 border border-obsidian-accent/30 rounded text-sm",
                            div { class: "font-semibold text-obsidian-accent mb-2", "Import complete" }
                            div { class: "text-xs text-obsidian-text-muted space-y-0.5",
                                div { "Journals created: {summary.journal_created}" }
                                div { "Generic notes created: {summary.generic_created}" }
                                if has_errors {
                                    div { class: "text-red-400 mt-2", "Errors: {summary.errors.len()}" }
                                    for e in &summary.errors {
                                        div { class: "font-mono text-[11px] text-red-300 pl-2", "{e}" }
                                    }
                                }
                            }
                            button {
                                class: "mt-3 text-xs text-obsidian-text-muted hover:text-obsidian-accent",
                                onclick: move |_| phase.set(ImportPhase::Idle),
                                "Done"
                            }
                        }
                    }
                } else { rsx! {} }
            }
        }
    }
}

#[component]
fn ImportPreview(
    summary: ImportPreviewSummary,
    skipped: Signal<HashSet<String>>,
    overrides: Signal<std::collections::HashMap<String, String>>,
) -> Element {
    rsx! {
        div { class: "text-xs text-obsidian-text-muted pb-2",
            span { class: "mr-3", "Journals: "
                span { class: "text-obsidian-accent font-semibold", "{summary.journal_count}" }
            }
            span { class: "mr-3", "Generic: "
                span { class: "text-obsidian-accent font-semibold", "{summary.generic_count}" }
            }
            if summary.error_count > 0 {
                span { class: "mr-3", "Errors: "
                    span { class: "text-red-400 font-semibold", "{summary.error_count}" }
                }
            }
        }

        if summary.rows.is_empty() {
            div { class: "p-6 rounded border border-white/5 bg-obsidian-bg/40 text-xs text-obsidian-text-muted text-center leading-relaxed",
                p { class: "font-semibold text-obsidian-text mb-1", "No markdown files found" }
                p {
                    "Nothing under " code { class: "font-mono", "{summary.root}" }
                    " looks like a note. Check that the path points at the vault root (the folder that contains your "
                    code { class: "font-mono", ".obsidian/" }
                    " directory), and that your notes use the "
                    code { class: "font-mono", ".md" }
                    " extension."
                }
            }
        } else {
            div { class: "max-h-[360px] overflow-y-auto rounded border border-white/5 divide-y divide-white/5",
                for row in summary.rows.iter().cloned() {
                    ImportPreviewRowItem {
                        key: "{row.path}",
                        row: row,
                        skipped: skipped,
                        overrides: overrides,
                    }
                }
            }
        }
    }
}

#[component]
fn ImportPreviewRowItem(
    row: ImportPreviewRow,
    skipped: Signal<HashSet<String>>,
    overrides: Signal<std::collections::HashMap<String, String>>,
) -> Element {
    let is_error = row.kind == "error";
    let path_key = row.path.clone();
    let path_for_skip = row.path.clone();
    let path_for_override = row.path.clone();

    let is_skipped = skipped.read().contains(&path_key) || is_error;
    let current_override = overrides.read().get(&path_key).cloned();
    let display_key = current_override.clone().unwrap_or_else(|| row.key.clone());

    let row_class = if is_error {
        "p-3 bg-red-900/10 opacity-70"
    } else if is_skipped {
        "p-3 opacity-40"
    } else {
        "p-3 hover:bg-white/[0.02]"
    };

    let kind_badge_class = match row.kind.as_str() {
        "journal" => "px-2 py-0.5 text-[10px] font-bold rounded bg-obsidian-accent/15 text-obsidian-accent border border-obsidian-accent/30 uppercase tracking-wide",
        "generic" => "px-2 py-0.5 text-[10px] font-bold rounded bg-white/5 text-obsidian-text border border-white/10 uppercase tracking-wide",
        _ => "px-2 py-0.5 text-[10px] font-bold rounded bg-red-900/30 text-red-400 border border-red-900/50 uppercase tracking-wide",
    };

    rsx! {
        div { class: "{row_class}",
            div { class: "flex items-start gap-3",
                if !is_error {
                    input {
                        r#type: "checkbox",
                        class: "mt-1 cursor-pointer accent-obsidian-accent shrink-0",
                        checked: !is_skipped,
                        onchange: move |e| {
                            let mut set = skipped.write();
                            if e.value() == "true" {
                                set.remove(&path_for_skip);
                            } else {
                                set.insert(path_for_skip.clone());
                            }
                        },
                    }
                }

                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 flex-wrap",
                        span { class: "{kind_badge_class}", "{row.kind}" }
                        span { class: "font-mono text-[11px] text-obsidian-text-muted truncate",
                            "{row.relative_path}"
                        }
                        if row.has_legacy_properties {
                            span {
                                class: "w-1.5 h-1.5 rounded-full bg-yellow-400",
                                title: "Frontmatter has non-native properties — preserved as legacy_properties",
                            }
                        }
                    }

                    if is_error {
                        div { class: "mt-1 text-xs text-red-400",
                            "{row.error.clone().unwrap_or_default()}"
                        }
                    } else {
                        // Key (date or title) — editable inline
                        div { class: "mt-1 flex items-center gap-2",
                            input {
                                class: "px-2 py-0.5 text-xs bg-obsidian-bg border border-white/5 rounded text-obsidian-text w-48 outline-none focus:border-obsidian-accent",
                                r#type: "text",
                                value: "{display_key}",
                                oninput: move |e| {
                                    let val = e.value();
                                    let mut map = overrides.write();
                                    if val == row.key {
                                        map.remove(&path_for_override);
                                    } else {
                                        map.insert(path_for_override.clone(), val);
                                    }
                                },
                            }
                            if !row.tags.is_empty() {
                                div { class: "flex flex-wrap gap-1",
                                    for tag in &row.tags {
                                        span { class: "px-1.5 py-0.5 text-[10px] rounded bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20",
                                            "#{tag}"
                                        }
                                    }
                                }
                            }
                        }

                        // Body preview
                        div { class: "mt-1 text-[11px] text-obsidian-text-muted line-clamp-2 leading-relaxed",
                            "{row.body_preview}"
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[component]
fn ExportFlow() -> Element {
    let mut target = use_signal(String::new);
    let mut phase = use_signal(|| ExportPhase::Idle);

    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg space-y-3",
            h3 { class: "text-sm font-bold text-obsidian-text", "Export to Obsidian" }
            p { class: "text-[11px] text-obsidian-text-muted",
                "Writes every journal entry to "
                code { class: "font-mono", "<target>/journal/YYYY-MM-DD.md" }
                " and every generic note to "
                code { class: "font-mono", "<target>/notes/<title>.md" }
                ". Existing files with matching names are overwritten."
            }

            if matches!(*phase.read(), ExportPhase::Idle | ExportPhase::Error(_)) {
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-3 py-2 bg-obsidian-bg border border-white/10 rounded-md text-sm text-obsidian-text font-mono placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent",
                        r#type: "text",
                        placeholder: "/path/to/export-target",
                        value: "{target}",
                        oninput: move |e| target.set(e.value()),
                    }
                    button {
                        class: "px-4 py-2 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-40",
                        disabled: target.read().trim().is_empty(),
                        onclick: move |_| {
                            let t = target.read().trim().to_string();
                            if t.is_empty() { return; }
                            phase.set(ExportPhase::Exporting);
                            spawn(async move {
                                match bridge::invoke_export_obsidian(&t).await {
                                    Ok(s) => phase.set(ExportPhase::Done {
                                        written: s.journal_written + s.generic_written,
                                        errors: s.errors.len(),
                                    }),
                                    Err(e) => phase.set(ExportPhase::Error(e)),
                                }
                            });
                        },
                        "Export"
                    }
                }

                if let ExportPhase::Error(err) = &*phase.read() {
                    div { class: "p-2 bg-red-900/20 text-red-400 border border-red-900/50 rounded text-xs",
                        "{err}"
                    }
                }
            }

            if matches!(*phase.read(), ExportPhase::Exporting) {
                div { class: "py-4 text-center text-obsidian-text-muted text-sm", "Writing files..." }
            }

            if let ExportPhase::Done { written, errors } = *phase.read() {
                div { class: "p-3 bg-obsidian-accent/10 border border-obsidian-accent/30 rounded text-sm",
                    div { class: "font-semibold text-obsidian-accent", "Export complete" }
                    div { class: "text-xs text-obsidian-text-muted mt-1",
                        "{written} files written"
                        if errors > 0 {
                            span { class: "text-red-400 ml-2", "({errors} errors)" }
                        }
                    }
                    button {
                        class: "mt-2 text-xs text-obsidian-text-muted hover:text-obsidian-accent",
                        onclick: move |_| phase.set(ExportPhase::Idle),
                        "Done"
                    }
                }
            }
        }
    }
}
