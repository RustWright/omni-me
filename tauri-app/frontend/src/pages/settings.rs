use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::{bridge, types::SyncStatus};

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

            // --- Danger Zone ---
            DangerZone {}
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
                                    match bridge::invoke_wipe_all_data().await {
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
