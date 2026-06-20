use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::{
    bridge,
    types::{AutoImportSourceView, SyncStatus},
};

#[component]
pub fn SettingsPage() -> Element {
    let mut server_url = use_signal(String::new);
    let mut device_id = use_signal(String::new);
    let mut sync_status = use_signal(|| None::<String>);
    let mut url_dirty = use_signal(|| false);

    // Timezone state
    let mut tz_signal: Signal<Tz> = use_context();
    let mut tz_input = use_signal(String::new);
    let mut tz_dirty = use_signal(|| false);
    let mut tz_status = use_signal(|| None::<String>);
    let mut tz_is_override = use_signal(|| false);

    // Load current settings on mount
    use_future(move || async move {
        if let Ok(info) = bridge::invoke_get_sync_info().await {
            server_url.set(info.server_url);
            device_id.set(info.device_id);
        }
        if let Ok(info) = bridge::invoke_get_timezone().await {
            tz_input.set(info.timezone);
            tz_is_override.set(info.is_override);
        }
    });

    rsx! {
        div { class: "max-w-3xl mx-auto w-full animate-in fade-in duration-300",

            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent mb-8", "Settings" }

            // --- Sync Section ---
            div { class: "mb-10 space-y-6",
                div { class: "border-b border-white/5 pb-2 mb-4",
                    h2 { class: "text-lg font-bold text-obsidian-text", "Cloud Synchronization" }
                }

                // Device ID (read-only)
                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Local Device ID" }
                    div { class: "p-3 bg-obsidian-sidebar/60 border border-white/5 rounded-lg font-mono text-xs text-obsidian-text-muted select-all",
                        "{device_id}"
                    }
                }

                // Server URL (editable)
                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Sync Server Address" }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
                            r#type: "text",
                            value: "{server_url}",
                            oninput: move |e| {
                                server_url.set(e.value().clone());
                                url_dirty.set(true);
                            },
                        }
                        if *url_dirty.read() {
                            button {
                                class: "px-4 py-2 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity",
                                onclick: move |_| {
                                    let url = server_url.read().clone();
                                    spawn(async move {
                                        match bridge::invoke_update_server_url(&url).await {
                                            Ok(_) => {
                                                url_dirty.set(false);
                                                sync_status.set(Some("Server configuration updated.".into()));
                                            }
                                            Err(e) => sync_status.set(Some(format!("Error: {e}"))),
                                        }
                                    });
                                },
                                "Update"
                            }
                        }
                    }
                }

                div { class: "pt-4",
                    button {
                        class: "w-full flex items-center justify-center gap-2 px-6 py-3 bg-white/5 border border-white/10 text-white font-bold rounded-lg hover:bg-white/10 transition-colors shadow-lg active:scale-[0.99]",
                        onclick: move |_| {
                            sync_status.set(Some("Initiating sync...".into()));
                                        spawn(async move {
                                            match bridge::invoke_trigger_sync().await {
                                                Ok(synced_status) => {
                                                    let SyncStatus{pulled, pushed} = synced_status;
                                                    sync_status.set(Some(format!("Sync successful: {pulled} down, {pushed} up.")));
                                                }
                                                Err(e) => sync_status.set(Some(format!("Sync failed: {e}"))),
                                            }
                                        });
                        },
                        svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" }
                        }
                        "Sync Now"
                    }
                }

                // Status display
                if let Some(status) = &*sync_status.read() {
                    div { class: "p-4 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded-lg text-sm text-obsidian-accent animate-in zoom-in-95 duration-200",
                        "{status}"
                    }
                }
            }

            // --- Timezone Section ---
            div { class: "mb-10 space-y-6",
                div { class: "border-b border-white/5 pb-2 mb-4",
                    h2 { class: "text-lg font-bold text-obsidian-text", "Timezone" }
                }

                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                        if *tz_is_override.read() { "Timezone (Manual Override)" }
                        else { "Timezone (Auto-detected)" }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "flex-1 px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
                            r#type: "text",
                            placeholder: "e.g. America/New_York",
                            value: "{tz_input}",
                            oninput: move |e| {
                                tz_input.set(e.value().clone());
                                tz_dirty.set(true);
                            },
                        }
                        if *tz_dirty.read() {
                            button {
                                class: "px-4 py-2 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity",
                                onclick: move |_| {
                                    let input = tz_input.read().clone();
                                    spawn(async move {
                                        match input.parse::<Tz>() {
                                            Ok(new_tz) => {
                                                match bridge::invoke_update_timezone(&input).await {
                                                    Ok(_) => {
                                                        tz_signal.set(new_tz);
                                                        tz_dirty.set(false);
                                                        tz_is_override.set(true);
                                                        tz_status.set(Some("Timezone updated.".into()));
                                                    }
                                                    Err(e) => tz_status.set(Some(format!("Error: {e}"))),
                                                }
                                            }
                                            Err(_) => tz_status.set(Some(format!("Invalid timezone: '{input}'. Use IANA format (e.g. America/New_York)."))),
                                        }
                                    });
                                },
                                "Update"
                            }
                        }
                    }
                }

                if *tz_is_override.read() {
                    button {
                        class: "text-sm text-obsidian-accent hover:underline",
                        onclick: move |_| {
                            spawn(async move {
                                match bridge::invoke_update_timezone("").await {
                                    Ok(_) => {
                                        if let Ok(info) = bridge::invoke_get_timezone().await {
                                            tz_input.set(info.timezone.clone());
                                            tz_is_override.set(info.is_override);
                                            if let Ok(tz) = info.timezone.parse::<Tz>() {
                                                tz_signal.set(tz);
                                            }
                                        }
                                        tz_dirty.set(false);
                                        tz_status.set(Some("Reset to auto-detected timezone.".into()));
                                    }
                                    Err(e) => tz_status.set(Some(format!("Error: {e}"))),
                                }
                            });
                        },
                        "Reset to auto-detected"
                    }
                }

                if let Some(status) = &*tz_status.read() {
                    div { class: "p-4 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded-lg text-sm text-obsidian-accent animate-in zoom-in-95 duration-200",
                        "{status}"
                    }
                }
            }

            // --- Base Currency (Phase 7.3) ---
            BaseCurrencySection {}

            // --- Obsidian Import / Export ---
            super::import_export::ImportExportSection {}

            // --- Attachment Cache (Phase 3.8) ---
            CacheSection {}

            // --- Auto-Import Sources (Phase 3.9) ---
            AutoImportSection {}

            // --- Accounts (3.9 auto-detected) ---
            AccountsSection {}

            // --- LLM Provider (3.8 bring-your-own-LLM) ---
            LlmProviderSection {}

            // --- Danger Zone ---
            DangerZone {}
        }
    }
}

/// Render a byte count as a short human-readable string. Uses binary units
/// (1024-step) because that's what filesystems actually report and the cap
/// in `commands::attachments` is expressed in MiB; mixing units here would
/// surface a confusing "you have 200 MB used, cap is 200 MB" off-by-decimal.
fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    }
}

/// ISO-4217 codes offered by the picker — the user's actual account currencies
/// first, then a few common ones. The backend accepts any 3-letter code, so this
/// list is just the convenient menu, not a hard constraint.
const CURRENCY_CODES: &[&str] = &["CAD", "USD", "EUR", "GBP", "NGN", "AUD", "JPY", "CHF"];

/// Base-currency picker (Phase 7.3). The selection persists server-side and is
/// read by the dashboard / accounts / budget aggregation as the FX base.
#[component]
fn BaseCurrencySection() -> Element {
    let mut current = use_signal(|| "CAD".to_string());
    let mut status = use_signal(|| None::<String>);

    use_future(move || async move {
        if let Ok(code) = bridge::invoke_get_base_currency().await {
            current.set(code);
        }
    });

    let selected = current.read().clone();

    rsx! {
        div { class: "mb-10 space-y-6",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "Base Currency" }
            }
            div {
                label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Currency for net worth, accounts, and budget totals"
                }
                select {
                    class: "px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text outline-none focus:border-obsidian-accent transition-colors",
                    value: "{selected}",
                    onchange: move |e| {
                        let code = e.value();
                        spawn(async move {
                            match bridge::invoke_update_base_currency(&code).await {
                                Ok(_) => {
                                    status.set(Some(format!("Base currency set to {code}.")));
                                    current.set(code);
                                }
                                Err(e) => status.set(Some(format!("Error: {e}"))),
                            }
                        });
                    },
                    for code in CURRENCY_CODES {
                        option { value: "{code}", "{code}" }
                    }
                }
                p { class: "text-xs text-obsidian-text-muted mt-2",
                    "Foreign-currency holdings are converted to this currency on the dashboard and accounts screens using your latest recorded FX rates."
                }
            }
            if let Some(s) = &*status.read() {
                div { class: "p-4 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded-lg text-sm text-obsidian-accent animate-in zoom-in-95 duration-200",
                    "{s}"
                }
            }
        }
    }
}

#[component]
fn CacheSection() -> Element {
    let mut size = use_signal(|| None::<u64>);
    let mut clearing = use_signal(|| false);
    let mut status = use_signal(|| None::<String>);

    let load_size = move || {
        spawn(async move {
            match bridge::invoke_attachment_cache_size().await {
                Ok(n) => size.set(Some(n)),
                Err(e) => status.set(Some(format!("Couldn't read cache size: {e}"))),
            }
        });
    };

    use_future(move || async move {
        if let Ok(n) = bridge::invoke_attachment_cache_size().await {
            size.set(Some(n));
        }
    });

    rsx! {
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "Attachment Cache" }
            }

            p { class: "text-sm text-obsidian-text-muted",
                "Captured receipt photos and statement PDFs are mirrored on this device for "
                "offline viewing. The cache is capped at 200 MiB and auto-evicts the least-recently-used entries; "
                "you can also clear it manually here. Server copies are unaffected — anything cleared here is "
                "re-fetched on demand."
            }

            div { class: "flex items-center justify-between gap-4 p-4 bg-obsidian-sidebar/60 border border-white/5 rounded-lg",
                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest block mb-1", "Used" }
                    div { class: "font-mono text-base text-obsidian-text",
                        match *size.read() {
                            Some(n) => format_bytes(n),
                            None => "…".to_string(),
                        }
                    }
                }
                button {
                    class: "px-4 py-2 bg-white/5 border border-white/10 text-obsidian-text font-semibold rounded-lg hover:bg-white/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed",
                    disabled: *clearing.read() || matches!(*size.read(), Some(0)),
                    onclick: move |_| {
                        clearing.set(true);
                        status.set(None);
                        spawn(async move {
                            match bridge::invoke_clear_attachment_cache().await {
                                Ok(freed) => {
                                    status.set(Some(format!("Cleared {} from cache.", format_bytes(freed))));
                                    size.set(Some(0));
                                }
                                Err(e) => status.set(Some(format!("Clear failed: {e}"))),
                            }
                            clearing.set(false);
                        });
                    },
                    if *clearing.read() { "Clearing…" } else { "Clear Cache" }
                }
            }

            button {
                class: "text-xs text-obsidian-text-muted hover:text-obsidian-accent hover:underline",
                onclick: move |_| load_size(),
                "Refresh size"
            }

            if let Some(s) = &*status.read() {
                div { class: "p-4 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded-lg text-sm text-obsidian-accent animate-in zoom-in-95 duration-200",
                    "{s}"
                }
            }
        }
    }
}

/// The phrase the user must type to authorize a full data wipe.
/// Shown verbatim in the instructions so the user knows what to enter.
const WIPE_CONFIRM_PHRASE: &str = "wipe everything zkqp";

/// Decides whether the final "Confirm Wipe" button should be enabled.
///
/// Called on every keystroke with the current `armed` flag and typed input.
/// Returning `true` arms the destructive click; `false` keeps it disabled.
fn is_wipe_confirmed(armed: bool, typed: &str) -> bool {
    armed && typed == WIPE_CONFIRM_PHRASE
}

#[component]
fn DangerZone() -> Element {
    let mut armed = use_signal(|| false);
    let mut confirm_input = use_signal(String::new);
    let mut wiping = use_signal(|| false);
    let mut wipe_status = use_signal(|| None::<String>);

    let can_commit = is_wipe_confirmed(*armed.read(), &confirm_input.read());

    rsx! {
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-red-900/40 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-red-400", "Danger Zone" }
            }

            p { class: "text-sm text-obsidian-text-muted",
                "Deletes every event and projection row on this device. "
                "Other devices are unaffected — they keep their data. "
                "If this device has previously synced, running Sync Now afterward "
                "will repopulate data from the server; turn off the server URL "
                "first if you want a permanent local-only reset. Cannot be undone."
            }

            if !*armed.read() {
                button {
                    class: "px-4 py-2 bg-red-900/20 text-red-400 border border-red-900/40 rounded-lg font-bold hover:bg-red-900/30 transition-colors",
                    onclick: move |_| {
                        armed.set(true);
                        wipe_status.set(None);
                    },
                    "Wipe all data…"
                }
            } else {
                div { class: "space-y-3 p-4 bg-red-900/10 border border-red-900/40 rounded-lg animate-in fade-in duration-200",
                    p { class: "text-sm text-red-300",
                        "Type "
                        span { class: "font-mono px-1.5 py-0.5 bg-red-900/30 rounded", "{WIPE_CONFIRM_PHRASE}" }
                        " to confirm."
                    }
                    input {
                        class: "w-full px-3 py-2 bg-obsidian-bg border border-red-900/40 rounded-lg text-obsidian-text outline-none focus:border-red-500 transition-colors font-mono text-sm",
                        r#type: "text",
                        value: "{confirm_input}",
                        autocomplete: "off",
                        autocorrect: "off",
                        autocapitalize: "off",
                        spellcheck: "false",
                        "data-1p-ignore": "true",
                        onpaste: move |e| { e.prevent_default(); },
                        oncut: move |e| { e.prevent_default(); },
                        ondrop: move |e| { e.prevent_default(); },
                        oninput: move |e| confirm_input.set(e.value()),
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "px-4 py-2 bg-red-600 text-white font-bold rounded-lg hover:bg-red-500 transition-colors disabled:opacity-40 disabled:hover:bg-red-600 disabled:cursor-not-allowed",
                            disabled: !can_commit || *wiping.read(),
                            onclick: move |_| {
                                wiping.set(true);
                                wipe_status.set(None);
                                spawn(async move {
                                    match bridge::invoke_wipe_all_data(WIPE_CONFIRM_PHRASE).await {
                                        Ok(_) => {
                                            wipe_status.set(Some("All local data wiped.".into()));
                                            armed.set(false);
                                            confirm_input.set(String::new());
                                        }
                                        Err(e) => wipe_status.set(Some(format!("Wipe failed: {e}"))),
                                    }
                                    wiping.set(false);
                                });
                            },
                            if *wiping.read() { "Wiping…" } else { "Confirm Wipe" }
                        }
                        button {
                            class: "px-4 py-2 bg-white/5 border border-white/10 text-obsidian-text font-semibold rounded-lg hover:bg-white/10 transition-colors",
                            onclick: move |_| {
                                armed.set(false);
                                confirm_input.set(String::new());
                                wipe_status.set(None);
                            },
                            "Cancel"
                        }
                    }
                }
            }

            if let Some(status) = &*wipe_status.read() {
                div { class: "p-4 bg-red-900/10 border border-red-900/30 rounded-lg text-sm text-red-300",
                    "{status}"
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Auto-Import Sources (Phase 3.9)
// ---------------------------------------------------------------------------

/// Render the "X ago" label for a `last_tick_at` ISO timestamp. Returns
/// "never" when the source hasn't ticked yet. Coarse buckets — auto-import
/// runs at 30-min cadence so second-level precision is noise.
fn format_relative_time(iso: Option<&str>) -> String {
    let Some(iso) = iso else {
        return "never".into();
    };
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let secs = chrono::Utc::now()
        .signed_duration_since(at.with_timezone(&chrono::Utc))
        .num_seconds()
        .max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Map the wire-format `health` string to a (label, badge-classes) pair.
/// `wire_health` is one of `"healthy" | "stale" | "degraded" | "unknown"`.
fn health_badge(wire_health: &str) -> (&'static str, &'static str) {
    match wire_health {
        "healthy" => (
            "Healthy",
            "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
        ),
        "stale" => (
            "Stale",
            "bg-amber-500/15 text-amber-400 border-amber-500/30",
        ),
        "degraded" => (
            "Degraded",
            "bg-red-500/15 text-red-400 border-red-500/30",
        ),
        _ => (
            "Unknown",
            "bg-white/5 text-obsidian-text-muted border-white/10",
        ),
    }
}

/// Render the one-line subtitle: outcome digest + relative tick time.
fn outcome_summary(outcome: &serde_json::Value) -> String {
    match outcome.get("kind").and_then(|k| k.as_str()) {
        Some("success") => {
            let n = outcome.get("events_appended").and_then(|v| v.as_u64()).unwrap_or(0);
            if n == 0 {
                "Last tick: no new events".into()
            } else {
                format!("Last tick: {n} events appended")
            }
        }
        Some("failure") => {
            let err = outcome.get("error").and_then(|v| v.as_str()).unwrap_or("(no error)");
            format!("Last tick failed: {err}")
        }
        _ => "Not yet run".into(),
    }
}

/// Source name from a config-definition JSON object.
fn config_name(def: &serde_json::Value) -> Option<String> {
    def.get("name").and_then(|v| v.as_str()).map(str::to_string)
}

/// One-line human summary of a config definition for the management list.
fn config_summary(def: &serde_json::Value) -> String {
    match def.get("type").and_then(|v| v.as_str()).unwrap_or("?") {
        "csv" => {
            let path = def.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let account = def.get("account").and_then(|v| v.as_str()).unwrap_or("");
            format!("CSV · {path} → {account}")
        }
        "subprocess" => {
            let cmd = def.get("command").and_then(|v| v.as_str()).unwrap_or("");
            format!("Subprocess · {cmd}")
        }
        other => other.to_string(),
    }
}

#[component]
fn AutoImportSection() -> Element {
    let mut sources: Signal<Option<Vec<AutoImportSourceView>>> = use_signal(|| None);
    let mut configs: Signal<Option<Vec<serde_json::Value>>> = use_signal(|| None);
    let mut loading_msg = use_signal(|| None::<String>);
    let ticking = use_signal(|| None::<String>);
    let mut show_form = use_signal(|| false);
    let mut edit_target = use_signal(|| None::<serde_json::Value>);

    // Re-pull both the *configured* definitions and the *running* status.
    let refresh = move || {
        spawn(async move {
            match bridge::invoke_list_auto_import_sources().await {
                Ok(list) => sources.set(Some(list)),
                Err(e) => loading_msg.set(Some(format!("Couldn't load running sources: {e}"))),
            }
            match bridge::invoke_list_source_configs().await {
                Ok(list) => configs.set(Some(list)),
                Err(e) => loading_msg.set(Some(format!("Couldn't load source configs: {e}"))),
            }
        });
    };

    use_future(move || async move {
        if let Ok(list) = bridge::invoke_list_auto_import_sources().await {
            sources.set(Some(list));
        }
        if let Ok(list) = bridge::invoke_list_source_configs().await {
            configs.set(Some(list));
        }
    });

    // By-name lookup of running status so a configured row can show its live
    // health — an added source spawns into the registry immediately, so it
    // appears here on the next refresh (live add/remove).
    let runtime_list = sources.read().clone().unwrap_or_default();
    let running: std::collections::HashMap<String, AutoImportSourceView> = runtime_list
        .iter()
        .map(|s| (s.name.clone(), s.clone()))
        .collect();

    rsx! {
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-white/5 pb-2 mb-4 flex items-center justify-between",
                h2 { class: "text-lg font-bold text-obsidian-text", "Auto-Import Sources" }
                button {
                    class: "text-xs text-obsidian-text-muted hover:text-obsidian-accent hover:underline",
                    onclick: move |_| refresh(),
                    "Refresh"
                }
            }

            p { class: "text-sm text-obsidian-text-muted",
                "Background pullers that import transactions from configured sources — "
                "CSV files and subprocess helpers (plus, in private builds, bank adapters). "
                "Add or remove generic sources below; changes save on the server and apply "
                "live — no restart."
            }

            // --- Configured sources (editable) ---
            div { class: "space-y-2",
                div { class: "flex items-center justify-between",
                    h3 { class: "text-sm font-semibold text-obsidian-text", "Configured sources" }
                    button {
                        class: "text-xs px-2 py-1 rounded bg-obsidian-accent/10 text-obsidian-accent hover:bg-obsidian-accent/20 transition-colors",
                        onclick: move |_| { edit_target.set(None); show_form.set(true); },
                        "+ Add source"
                    }
                }

                if *show_form.read() {
                    AddSourceForm {
                        initial: edit_target.read().clone(),
                        on_saved: move |_: ()| {
                            show_form.set(false);
                            edit_target.set(None);
                            loading_msg.set(Some("Saved — applied live.".into()));
                            refresh();
                        },
                        on_cancel: move |_: ()| { show_form.set(false); edit_target.set(None); },
                    }
                }

                match configs.read().as_ref() {
                    None => rsx! {
                        div { class: "text-sm text-obsidian-text-muted italic", "Loading…" }
                    },
                    Some(list) if list.is_empty() => rsx! {
                        div { class: "p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg text-sm text-obsidian-text-muted",
                            "No generic sources configured yet. Use “+ Add source” to declare a CSV file or a subprocess helper."
                        }
                    },
                    Some(list) => {
                        let defs = list.clone();
                        rsx! {
                            div { class: "space-y-2",
                                for def in defs.into_iter() {
                                    {
                                        let nm = config_name(&def);
                                        let run = nm.as_ref().and_then(|n| running.get(n)).cloned();
                                        rsx! {
                                            ConfiguredSourceRow {
                                                def: def.clone(),
                                                running_health: run.map(|s| s.health),
                                                on_edit: move |d: serde_json::Value| {
                                                    edit_target.set(Some(d));
                                                    show_form.set(true);
                                                },
                                                on_removed: move |_: ()| {
                                                    loading_msg.set(Some("Removed — applied live.".into()));
                                                    refresh();
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

            // --- Running now (live status) ---
            div { class: "space-y-2 pt-2",
                h3 { class: "text-sm font-semibold text-obsidian-text", "Running now" }
                match &*sources.read() {
                    None => rsx! {
                        div { class: "text-sm text-obsidian-text-muted italic", "Loading…" }
                    },
                    Some(list) if list.is_empty() => rsx! {
                        div { class: "p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg text-sm text-obsidian-text-muted",
                            "Nothing running yet. Added sources start ticking immediately; built-in bank sources (private builds) appear here once they tick."
                        }
                    },
                    Some(list) => {
                        let rows = list.clone();
                        rsx! {
                            div { class: "space-y-2",
                                for src in rows.into_iter() {
                                    AutoImportRow {
                                        source: src.clone(),
                                        ticking_now: ticking.read().as_deref() == Some(src.name.as_str()),
                                        on_tick: {
                                            let name = src.name.clone();
                                            let mut ticking = ticking;
                                            let mut sources = sources;
                                            let mut loading_msg = loading_msg;
                                            move |_: ()| {
                                                let name = name.clone();
                                                ticking.set(Some(name.clone()));
                                                spawn(async move {
                                                    let result = bridge::invoke_trigger_auto_import_tick(&name).await;
                                                    match result {
                                                        Ok(r) => loading_msg.set(Some(format!(
                                                            "Manual tick on '{name}' appended {} events.",
                                                            r.events_appended
                                                        ))),
                                                        Err(e) => loading_msg.set(Some(format!(
                                                            "Manual tick on '{name}' failed: {e}"
                                                        ))),
                                                    }
                                                    ticking.set(None);
                                                    // Re-pull status after the tick so the
                                                    // row reflects the new last_outcome.
                                                    if let Ok(list) = bridge::invoke_list_auto_import_sources().await {
                                                        sources.set(Some(list));
                                                    }
                                                });
                                            }
                                        },
                                        on_reauth_success: {
                                            let mut sources = sources;
                                            move |_: ()| {
                                                // A successful reconnect clears the source's
                                                // NeedsReauth server-side; re-pull so the row
                                                // drops back to its normal health state.
                                                spawn(async move {
                                                    if let Ok(list) = bridge::invoke_list_auto_import_sources().await {
                                                        sources.set(Some(list));
                                                    }
                                                });
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(msg) = &*loading_msg.read() {
                div { class: "p-3 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded-lg text-xs text-obsidian-accent animate-in zoom-in-95 duration-200",
                    "{msg}"
                }
            }
        }
    }
}

/// Accounts section (3.9 auto-detected). The balance-bearing accounts
/// (Assets / Liabilities / Unmatched) are derived automatically from the ledger
/// — there's no list to maintain. This section is overrides-only: rename an
/// account for a friendlier label, or hide one from the Accounts screen + net
/// worth. Applies live (it emits an `account_added` override event).
#[component]
fn AccountsSection() -> Element {
    let mut accounts = use_signal(Vec::<bridge::DetectedAccountView>::new);
    let mut loaded = use_signal(|| false);

    use_future(move || async move {
        if let Ok(list) = bridge::invoke_list_detected_accounts().await {
            accounts.set(list);
        }
        loaded.set(true);
    });

    rsx! {
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "Accounts" }
            }
            p { class: "text-sm text-obsidian-text-muted",
                "Your asset & liability accounts are detected automatically from your ledger — "
                "nothing to add or maintain. Rename one for a friendlier label, or hide an "
                "account you don't want counted on the Accounts screen or in net worth."
            }

            if !*loaded.read() {
                p { class: "text-xs text-obsidian-text-muted", "Loading…" }
            } else if accounts.read().is_empty() {
                p { class: "text-xs text-obsidian-text-muted",
                    "No accounts yet — they appear here once transactions reference them."
                }
            } else {
                div { class: "space-y-1.5",
                    for acct in accounts.read().iter().cloned() {
                        AccountOverrideRow {
                            key: "{acct.account}",
                            account: acct.account.clone(),
                            display_name: acct.display_name.clone(),
                            hidden: acct.hidden,
                            on_changed: move |_| {
                                spawn(async move {
                                    if let Ok(list) = bridge::invoke_list_detected_accounts().await {
                                        accounts.set(list);
                                    }
                                });
                            },
                        }
                    }
                }
            }
        }
    }
}

/// One detected account row: a rename field + a Hide/Unhide toggle. Saves via
/// `set_account_override` and asks the parent to reload so the list reflects the
/// new state. Hidden rows render dimmed with an "Unhide" affordance.
#[component]
fn AccountOverrideRow(
    account: String,
    display_name: Option<String>,
    hidden: bool,
    on_changed: EventHandler<()>,
) -> Element {
    let mut name_input = use_signal(|| display_name.clone().unwrap_or_default());
    let mut saving = use_signal(|| false);
    let row_dim = if hidden { "opacity-50" } else { "" };

    let inp = "flex-1 min-w-0 px-2 py-1 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none disabled:opacity-50";

    // Current rename value → Option (empty = clear the override label).
    let current_name = move || {
        let v = name_input.read().trim().to_string();
        if v.is_empty() { None } else { Some(v) }
    };

    rsx! {
        div { class: "flex items-center gap-2 p-2 rounded border border-white/5 bg-obsidian-sidebar/40 {row_dim}",
            div { class: "flex-1 min-w-0",
                input {
                    class: inp,
                    placeholder: "{account}",
                    value: "{name_input}",
                    disabled: *saving.read(),
                    oninput: move |e| name_input.set(e.value()),
                    onchange: {
                        let account = account.clone();
                        move |_| {
                            let account = account.clone();
                            let dn = current_name();
                            saving.set(true);
                            spawn(async move {
                                let _ = bridge::invoke_set_account_override(account, dn, hidden).await;
                                saving.set(false);
                                on_changed.call(());
                            });
                        }
                    },
                }
                div { class: "font-mono text-[10px] text-obsidian-text-muted/70 truncate mt-0.5", "{account}" }
            }
            button {
                class: "shrink-0 px-2.5 py-1 bg-white/5 border border-white/10 text-obsidian-text text-xs font-semibold rounded hover:bg-white/10 transition-colors disabled:opacity-40",
                disabled: *saving.read(),
                onclick: {
                    let account = account.clone();
                    move |_| {
                        let account = account.clone();
                        let dn = current_name();
                        saving.set(true);
                        spawn(async move {
                            let _ = bridge::invoke_set_account_override(account, dn, !hidden).await;
                            saving.set(false);
                            on_changed.call(());
                        });
                    }
                },
                if hidden { "Unhide" } else { "Hide" }
            }
        }
    }
}

/// LLM provider picker (3.8 bring-your-own-LLM). Gemini by default; pick an
/// OpenAI-compatible endpoint (Ollama / llama.cpp / vLLM / commercial) to bring
/// your own. Restart-to-apply — the running client is chosen at boot — unlike
/// the auto-import sources above, which apply live. The api_key is write-only:
/// it's never read back (the form shows "key configured" via `has_key`), and a
/// blank field on save preserves the stored key.
#[component]
fn LlmProviderSection() -> Element {
    let mut provider = use_signal(|| "gemini".to_string());
    let mut base_url = use_signal(String::new);
    let mut model = use_signal(String::new);
    let mut api_key = use_signal(String::new); // never prefilled (write-only)
    let mut has_key = use_signal(|| false);
    let mut vision = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut msg = use_signal(|| None::<String>);

    use_future(move || async move {
        if let Ok(cfg) = bridge::invoke_get_llm_config().await {
            provider.set(cfg.provider);
            base_url.set(cfg.base_url.unwrap_or_default());
            model.set(cfg.model.unwrap_or_default());
            has_key.set(cfg.has_key);
            vision.set(cfg.vision);
        }
    });

    let lbl = "block text-xs text-obsidian-text-muted mb-1";
    let inp = "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none";

    let is_openai = provider.read().as_str() == "openai_compatible";
    let key_placeholder = if *has_key.read() {
        "•••••••• (leave blank to keep current)"
    } else {
        "sk-… (blank is fine for local servers)"
    };

    let submit = move |_| {
        let body = serde_json::json!({
            "provider": provider.read().clone(),
            "base_url": base_url.read().trim().to_string(),
            "model": model.read().trim().to_string(),
            "api_key": api_key.read().clone(),
            "vision": *vision.read(),
        });
        let had_key_input = !api_key.read().trim().is_empty();
        saving.set(true);
        msg.set(None);
        spawn(async move {
            match bridge::invoke_set_llm_config(body).await {
                Ok(()) => {
                    msg.set(Some("Saved. Applies on the next server restart.".into()));
                    if had_key_input {
                        has_key.set(true);
                    }
                    api_key.set(String::new());
                }
                Err(e) => msg.set(Some(format!("Save failed: {e}"))),
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "LLM Provider" }
            }
            p { class: "text-sm text-obsidian-text-muted",
                "Which model processes your notes — tagging, task and expense extraction. The "
                "default is Google Gemini; point it at any OpenAI-compatible endpoint (Ollama, "
                "llama.cpp, vLLM, or a commercial API) to bring your own. Document extraction "
                "(receipts & statements) stays on Gemini unless you opt the endpoint in below."
            }

            div {
                label { class: lbl, "Provider" }
                select {
                    class: inp,
                    value: "{provider}",
                    onchange: move |e| provider.set(e.value()),
                    option { value: "gemini", "Google Gemini (default)" }
                    option { value: "openai_compatible", "OpenAI-compatible" }
                }
            }

            if is_openai {
                div { class: "space-y-3",
                    div {
                        label { class: lbl, "Base URL" }
                        input { class: inp, placeholder: "http://localhost:11434/v1", value: "{base_url}", oninput: move |e| base_url.set(e.value()) }
                    }
                    div {
                        label { class: lbl, "Model" }
                        input { class: inp, placeholder: "llama3.1", value: "{model}", oninput: move |e| model.set(e.value()) }
                    }
                    div {
                        label { class: lbl, "API key" }
                        input {
                            class: inp,
                            r#type: "password",
                            placeholder: key_placeholder,
                            value: "{api_key}",
                            oninput: move |e| api_key.set(e.value()),
                        }
                    }
                    label { class: "flex items-start gap-2 cursor-pointer select-none",
                        input {
                            r#type: "checkbox",
                            class: "mt-0.5",
                            checked: *vision.read(),
                            onchange: move |e| vision.set(e.checked()),
                        }
                        span { class: "text-xs text-obsidian-text-muted",
                            "Also use this endpoint to read receipts & statements (vision). "
                            "Leave off if it has no image support — extraction stays on Gemini."
                        }
                    }
                }
            }

            div { class: "flex items-center gap-3",
                button {
                    class: "px-3 py-1.5 rounded bg-obsidian-accent text-obsidian-bg text-sm font-medium disabled:opacity-60",
                    disabled: *saving.read(),
                    onclick: submit,
                    if *saving.read() { "Saving…" } else { "Save" }
                }
                span { class: "text-xs text-obsidian-text-muted", "Applies on the next server restart." }
            }

            if let Some(m) = &*msg.read() {
                div { class: "p-3 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded-lg text-xs text-obsidian-accent",
                    "{m}"
                }
            }
        }
    }
}

#[component]
fn AutoImportRow(
    source: AutoImportSourceView,
    ticking_now: bool,
    on_tick: EventHandler<()>,
    on_reauth_success: EventHandler<()>,
) -> Element {
    let (label, badge_classes) = health_badge(&source.health);
    let relative = format_relative_time(source.last_tick_at.as_deref());
    let summary = outcome_summary(&source.last_outcome);

    // Re-auth is orthogonal to health: `health` answers "is data flowing" (a
    // transient blip you wait out), while `auth_state` answers "must the user
    // act". A `needs_reauth` source surfaces an inline Reconnect affordance so
    // the one-time code can be entered in-app — never via SSH to the host.
    let needs_reauth =
        source.auth_state.get("kind").and_then(|k| k.as_str()) == Some("needs_reauth");
    let reauth_reason = source
        .auth_state
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("This source's session has expired — reconnect to resume importing.")
        .to_string();
    let can_reauth = needs_reauth && source.reauth_capable;
    let name = source.name.clone();

    let mut show_otp = use_signal(|| false);
    let mut otp = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    // (is_error, text) — green confirmation vs red rejection/error.
    let mut msg = use_signal(|| None::<(bool, String)>);

    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/5 rounded-lg",
            div { class: "flex items-start justify-between gap-4 mb-2",
                div { class: "min-w-0 flex-1",
                    div { class: "flex items-center gap-2 mb-1",
                        span { class: "font-mono text-sm text-obsidian-text truncate", "{source.name}" }
                        span {
                            class: "text-[10px] font-bold uppercase tracking-widest px-2 py-0.5 rounded border {badge_classes}",
                            "{label}"
                        }
                    }
                    div { class: "text-xs text-obsidian-text-muted", "{summary}" }
                    div { class: "text-[10px] text-obsidian-text-muted/70 mt-1 font-mono",
                        "Last tick: {relative} · interval: {source.interval_secs / 60}m"
                    }
                }
                button {
                    class: "shrink-0 px-3 py-1.5 bg-white/5 border border-white/10 text-obsidian-text text-xs font-semibold rounded hover:bg-white/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed",
                    disabled: ticking_now,
                    onclick: move |_| on_tick.call(()),
                    if ticking_now { "Fetching…" } else { "Fetch now" }
                }
            }

            if needs_reauth {
                div { class: "mt-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/30 space-y-2",
                    div { class: "flex items-start justify-between gap-3",
                        div { class: "min-w-0",
                            div { class: "text-[10px] font-bold uppercase tracking-widest text-amber-400 mb-0.5",
                                "Reconnect needed"
                            }
                            div { class: "text-xs text-obsidian-text-muted", "{reauth_reason}" }
                        }
                        if can_reauth && !*show_otp.read() {
                            button {
                                class: "shrink-0 px-3 py-1.5 bg-amber-500/20 border border-amber-500/40 text-amber-300 text-xs font-semibold rounded hover:bg-amber-500/30 transition-colors",
                                onclick: move |_| {
                                    show_otp.set(true);
                                    msg.set(None);
                                },
                                "Reconnect"
                            }
                        }
                    }

                    if can_reauth && *show_otp.read() {
                        div { class: "space-y-2",
                            label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest block",
                                "Authenticator code"
                            }
                            div { class: "flex items-center gap-2",
                                input {
                                    class: "w-32 px-3 py-1.5 bg-obsidian-sidebar border border-white/15 rounded text-obsidian-text text-sm font-mono tracking-[0.4em] text-center outline-none focus:border-obsidian-accent transition-colors disabled:opacity-40",
                                    r#type: "text",
                                    inputmode: "numeric",
                                    autocomplete: "one-time-code",
                                    maxlength: "6",
                                    placeholder: "······",
                                    value: "{otp}",
                                    disabled: *submitting.read(),
                                    oninput: move |e| {
                                        // Keep digits only, cap at 6 — TOTP is a 6-digit code.
                                        let cleaned: String =
                                            e.value().chars().filter(|c| c.is_ascii_digit()).take(6).collect();
                                        otp.set(cleaned);
                                    },
                                }
                                button {
                                    class: "px-3 py-1.5 bg-obsidian-accent text-white text-xs font-bold rounded hover:opacity-90 transition-opacity disabled:opacity-40 disabled:cursor-not-allowed",
                                    disabled: *submitting.read() || otp.read().len() != 6,
                                    onclick: {
                                        let name = name.clone();
                                        move |_| {
                                            let name = name.clone();
                                            let code = otp.read().clone();
                                            submitting.set(true);
                                            msg.set(None);
                                            spawn(async move {
                                                let result = bridge::invoke_reauth_source(&name, &code).await;
                                                submitting.set(false);
                                                match result {
                                                    Ok(outcome) => match outcome
                                                        .get("status")
                                                        .and_then(|s| s.as_str())
                                                    {
                                                        Some("active") => {
                                                            msg.set(Some((
                                                                false,
                                                                "Reconnected — session refreshed.".into(),
                                                            )));
                                                            otp.set(String::new());
                                                            show_otp.set(false);
                                                            on_reauth_success.call(());
                                                        }
                                                        Some("invalid_otp") => {
                                                            msg.set(Some((
                                                                true,
                                                                "Authenticator code rejected — try again.".into(),
                                                            )));
                                                            otp.set(String::new());
                                                        }
                                                        Some("not_supported") => {
                                                            msg.set(Some((
                                                                true,
                                                                "This source can't be reconnected from here.".into(),
                                                            )));
                                                        }
                                                        _ => {
                                                            let m = outcome
                                                                .get("message")
                                                                .and_then(|m| m.as_str())
                                                                .unwrap_or("Reconnect failed — please try again.")
                                                                .to_string();
                                                            msg.set(Some((true, m)));
                                                        }
                                                    },
                                                    Err(e) => {
                                                        msg.set(Some((true, format!("Reconnect failed: {e}"))))
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    if *submitting.read() { "Reconnecting…" } else { "Submit" }
                                }
                                button {
                                    class: "px-2 py-1.5 text-xs text-obsidian-text-muted hover:text-obsidian-text disabled:opacity-40",
                                    disabled: *submitting.read(),
                                    onclick: move |_| {
                                        show_otp.set(false);
                                        otp.set(String::new());
                                        msg.set(None);
                                    },
                                    "Cancel"
                                }
                            }
                        }
                    }

                    if needs_reauth && !source.reauth_capable {
                        div { class: "text-[10px] text-obsidian-text-muted/70",
                            "This source can't be reconnected from the app — check its credentials on the server."
                        }
                    }

                    if let Some((is_err, text)) = &*msg.read() {
                        div {
                            class: if *is_err { "text-[11px] text-red-400" } else { "text-[11px] text-emerald-400" },
                            "{text}"
                        }
                    }
                }
            }
        }
    }
}

/// One row in the *Configured sources* list (3.7). Shows the definition's name,
/// type summary, and its live status — a running source borrows the live health
/// badge; one that isn't currently running (e.g. disabled) shows a neutral
/// "not running". Offers Edit (re-open the form prefilled) + Remove.
#[component]
fn ConfiguredSourceRow(
    def: serde_json::Value,
    running_health: Option<String>,
    on_edit: EventHandler<serde_json::Value>,
    on_removed: EventHandler<()>,
) -> Element {
    let name = config_name(&def).unwrap_or_else(|| "(unnamed)".into());
    let summary = config_summary(&def);
    let enabled = def.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let mut removing = use_signal(|| false);
    let mut err = use_signal(|| None::<String>);

    let (badge_label, badge_classes): (String, &'static str) = match &running_health {
        Some(h) => {
            let (label, classes) = health_badge(h);
            (label.to_string(), classes)
        }
        None => (
            "not running".into(),
            "bg-white/5 text-obsidian-text-muted border border-white/10",
        ),
    };

    rsx! {
        div { class: "p-3 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
            div { class: "flex items-center justify-between gap-2",
                div { class: "min-w-0",
                    div { class: "flex items-center gap-2",
                        span { class: "font-mono text-sm text-obsidian-text truncate", "{name}" }
                        span { class: "text-[10px] px-1.5 py-0.5 rounded {badge_classes}", "{badge_label}" }
                        if !enabled {
                            span { class: "text-[10px] px-1.5 py-0.5 rounded bg-white/5 text-obsidian-text-muted",
                                "disabled"
                            }
                        }
                    }
                    div { class: "text-xs text-obsidian-text-muted truncate mt-0.5", "{summary}" }
                }
                div { class: "flex items-center gap-1 shrink-0",
                    button {
                        class: "text-xs px-2 py-1 rounded hover:bg-white/5 text-obsidian-text-muted hover:text-obsidian-text transition-colors",
                        onclick: {
                            let def = def.clone();
                            move |_| on_edit.call(def.clone())
                        },
                        "Edit"
                    }
                    button {
                        class: "text-xs px-2 py-1 rounded hover:bg-red-500/10 text-obsidian-text-muted hover:text-red-300 transition-colors disabled:opacity-50",
                        disabled: *removing.read(),
                        onclick: {
                            let name = name.clone();
                            move |_| {
                                let name = name.clone();
                                removing.set(true);
                                err.set(None);
                                spawn(async move {
                                    match bridge::invoke_remove_source_config(&name).await {
                                        Ok(()) => on_removed.call(()),
                                        Err(e) => {
                                            err.set(Some(e));
                                            removing.set(false);
                                        }
                                    }
                                });
                            }
                        },
                        if *removing.read() { "Removing…" } else { "Remove" }
                    }
                }
            }
            if let Some(e) = &*err.read() {
                div { class: "text-xs text-red-300 mt-1", "{e}" }
            }
        }
    }
}

/// Add / edit a config-driven source (3.7). Builds the source definition as an
/// untyped JSON object (the client crate has no `core::auto_import` types) and
/// posts it; the server validates + persists. `initial = Some(def)` is edit
/// mode (the name is the key, so it's locked); `None` is add mode.
#[component]
fn AddSourceForm(
    initial: Option<serde_json::Value>,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let is_edit = initial.is_some();
    let init = initial.clone().unwrap_or_else(|| serde_json::json!({}));
    let g = |k: &str| {
        init.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let gcol = |k: &str| {
        init.get("columns")
            .and_then(|c| c.get(k))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let or = |s: String, dflt: &str| if s.is_empty() { dflt.to_string() } else { s };

    let mut name = use_signal(|| g("name"));
    let mut kind = use_signal(|| or(g("type"), "csv"));
    // csv fields
    let mut path = use_signal(|| g("path"));
    let mut account = use_signal(|| g("account"));
    let mut commodity = use_signal(|| or(g("commodity"), "CAD"));
    let mut has_header =
        use_signal(|| init.get("has_header").and_then(|v| v.as_bool()).unwrap_or(true));
    let mut date_format = use_signal(|| or(g("date_format"), "%Y-%m-%d"));
    let mut col_date = use_signal(|| or(gcol("date"), "Date"));
    let mut col_amount = use_signal(|| or(gcol("amount"), "Amount"));
    let mut col_desc = use_signal(|| or(gcol("description"), "Description"));
    let mut col_id = use_signal(|| gcol("id"));
    // subprocess fields
    let mut command = use_signal(|| g("command"));
    let mut args = use_signal(|| {
        init.get("args")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    });

    let mut saving = use_signal(|| false);
    let mut err = use_signal(|| None::<String>);

    let lbl = "block text-xs text-obsidian-text-muted mb-1";
    let inp = "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-sm text-obsidian-text placeholder:text-obsidian-text-muted focus:border-obsidian-accent/60 focus:outline-none";

    let submit = move |_| {
        let nm = name.read().trim().to_string();
        if nm.is_empty() {
            err.set(Some("Name is required.".into()));
            return;
        }
        if !nm
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            err.set(Some(
                "Name may contain only letters, numbers, dashes, and underscores.".into(),
            ));
            return;
        }

        let source = if kind.read().as_str() == "csv" {
            let path_v = path.read().trim().to_string();
            let account_v = account.read().trim().to_string();
            if path_v.is_empty() {
                err.set(Some("File path is required.".into()));
                return;
            }
            if account_v.is_empty() {
                err.set(Some("Account is required.".into()));
                return;
            }
            let mut cols = serde_json::json!({
                "date": col_date.read().trim(),
                "amount": col_amount.read().trim(),
                "description": col_desc.read().trim(),
            });
            let id_v = col_id.read().trim().to_string();
            if !id_v.is_empty() {
                cols["id"] = serde_json::json!(id_v);
            }
            serde_json::json!({
                "name": nm,
                "type": "csv",
                "enabled": true,
                "path": path_v,
                "account": account_v,
                "commodity": or(commodity.read().trim().to_string(), "CAD"),
                "has_header": *has_header.read(),
                "date_format": or(date_format.read().trim().to_string(), "%Y-%m-%d"),
                "columns": cols,
            })
        } else {
            let command_v = command.read().trim().to_string();
            if command_v.is_empty() {
                err.set(Some("Command is required.".into()));
                return;
            }
            let arg_vec: Vec<String> =
                args.read().split_whitespace().map(str::to_string).collect();
            serde_json::json!({
                "name": nm,
                "type": "subprocess",
                "enabled": true,
                "command": command_v,
                "args": arg_vec,
            })
        };

        saving.set(true);
        err.set(None);
        spawn(async move {
            match bridge::invoke_add_source_config(source).await {
                Ok(()) => on_saved.call(()),
                Err(e) => {
                    err.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-obsidian-accent/30 rounded-lg space-y-3",
            div { class: "text-sm font-semibold text-obsidian-text",
                if is_edit { "Edit source" } else { "Add source" }
            }

            div { class: "grid grid-cols-2 gap-3",
                div {
                    label { class: lbl, "Name" }
                    input {
                        class: inp,
                        r#type: "text",
                        placeholder: "my-checking",
                        value: "{name}",
                        disabled: is_edit,
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    label { class: lbl, "Type" }
                    select {
                        class: inp,
                        value: "{kind}",
                        onchange: move |e| kind.set(e.value()),
                        option { value: "csv", "CSV file" }
                        option { value: "subprocess", "Subprocess helper" }
                    }
                }
            }

            if kind.read().as_str() == "csv" {
                div { class: "space-y-3",
                    div {
                        label { class: lbl, "File path (on server)" }
                        input { class: inp, placeholder: "/data/imports/checking.csv", value: "{path}", oninput: move |e| path.set(e.value()) }
                    }
                    div { class: "grid grid-cols-2 gap-3",
                        div {
                            label { class: lbl, "Account" }
                            input { class: inp, placeholder: "Assets:Bank:Chequing", value: "{account}", oninput: move |e| account.set(e.value()) }
                        }
                        div {
                            label { class: lbl, "Commodity" }
                            input { class: inp, placeholder: "CAD", value: "{commodity}", oninput: move |e| commodity.set(e.value()) }
                        }
                    }
                    div { class: "grid grid-cols-3 gap-3",
                        div {
                            label { class: lbl, "Date column" }
                            input { class: inp, value: "{col_date}", oninput: move |e| col_date.set(e.value()) }
                        }
                        div {
                            label { class: lbl, "Amount column" }
                            input { class: inp, value: "{col_amount}", oninput: move |e| col_amount.set(e.value()) }
                        }
                        div {
                            label { class: lbl, "Description column" }
                            input { class: inp, value: "{col_desc}", oninput: move |e| col_desc.set(e.value()) }
                        }
                    }
                    div { class: "grid grid-cols-2 gap-3",
                        div {
                            label { class: lbl, "Id column (optional)" }
                            input { class: inp, placeholder: "Ref", value: "{col_id}", oninput: move |e| col_id.set(e.value()) }
                        }
                        div {
                            label { class: lbl, "Date format" }
                            input { class: inp, placeholder: "%Y-%m-%d", value: "{date_format}", oninput: move |e| date_format.set(e.value()) }
                        }
                    }
                    label { class: "flex items-center gap-2 text-xs text-obsidian-text-muted",
                        input {
                            r#type: "checkbox",
                            checked: *has_header.read(),
                            onchange: move |e| has_header.set(e.value() == "true"),
                        }
                        "File has a header row"
                    }
                }
            } else {
                div { class: "space-y-3",
                    div {
                        label { class: lbl, "Command" }
                        input { class: inp, placeholder: "/opt/omni/helpers/my-scraper", value: "{command}", oninput: move |e| command.set(e.value()) }
                    }
                    div {
                        label { class: lbl, "Args (space-separated)" }
                        input { class: inp, placeholder: "--since 30d", value: "{args}", oninput: move |e| args.set(e.value()) }
                    }
                }
            }

            div { class: "text-xs text-obsidian-text-muted",
                "Saved to the server's "
                span { class: "font-mono", "sources.toml" }
                " and applied live — new and changed sources start ticking immediately."
            }

            if let Some(e) = &*err.read() {
                div { class: "text-xs text-red-300", "{e}" }
            }

            div { class: "flex items-center gap-2",
                button {
                    class: "px-3 py-1.5 rounded bg-obsidian-accent text-obsidian-bg text-sm font-medium disabled:opacity-60",
                    disabled: *saving.read(),
                    onclick: submit,
                    if *saving.read() { "Saving…" } else { "Save" }
                }
                button {
                    class: "px-3 py-1.5 rounded text-sm text-obsidian-text-muted hover:text-obsidian-text transition-colors",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}

