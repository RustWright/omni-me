use dioxus::prelude::*;

use crate::{bridge, types::SyncStatus};

#[component]
pub fn SettingsPage() -> Element {
    let mut server_url = use_signal(|| String::new());
    let mut device_id = use_signal(|| String::new());
    let mut sync_status = use_signal(|| None::<String>);
    let mut url_dirty = use_signal(|| false);

    // Load current sync info on mount
    use_future(move || async move {
        if let Ok(info) = bridge::invoke_get_sync_info().await {
            server_url.set(info.server_url);
            device_id.set(info.device_id);
        }
    });

    rsx! {
        div {
            style: "max-width: 720px; margin: 0 auto;",

            h1 {
                style: "
                    font-size: 24px;
                    font-weight: 600;
                    margin: 0 0 24px 0;
                    color: #1a1a2e;
                ",
                "Settings"
            }

            // --- Sync Section ---
            div {
                style: "margin-bottom: 24px;",

                h2 {
                    style: "font-size: 18px; font-weight: 600; margin: 0 0 12px 0; color: #1a1a2e;",
                    "Sync"
                }

                // Device ID (read-only)
                div {
                    style: "margin-bottom: 16px;",
                    label {
                        style: "display: block; font-size: 14px; color: #666; margin-bottom: 4px;",
                        "Device ID"
                    }
                    div {
                        style: "
                            padding: 8px 12px;
                            background: #f5f5f5;
                            border-radius: 6px;
                            font-family: monospace;
                            font-size: 13px;
                            color: #333;
                        ",
                        "{device_id}"
                    }
                }

                // Server URL (editable)
                div {
                    style: "margin-bottom: 16px;",
                    label {
                        style: "display: block; font-size: 14px; color: #666; margin-bottom: 4px;",
                        "Server URL"
                    }
                    div {
                        style: "display: flex; gap: 8px;",
                        input {
                            style: "
                                flex: 1;
                                padding: 8px 12px;
                                border: 1px solid #ddd;
                                border-radius: 6px;
                                font-size: 14px;
                            ",
                            r#type: "text",
                            value: "{server_url}",
                            oninput: move |e| {
                                server_url.set(e.value().clone());
                                url_dirty.set(true);
                            },
                        }
                        if *url_dirty.read() {
                            button {
                                style: "
                                    padding: 8px 16px;
                                    background: #4a90d9;
                                    color: white;
                                    border: none;
                                    border-radius: 6px;
                                    cursor: pointer;
                                    font-size: 14px;
                                ",
                                onclick: move |_| {
                                    let url = server_url.read().clone();
                                    spawn(async move {
                                        match bridge::invoke_update_server_url(&url).await {
                                            Ok(_) => {
                                                url_dirty.set(false);
                                                sync_status.set(Some("Server URL saved".into()));
                                            }
                                            Err(e) => sync_status.set(Some(format!("Error: {e}"))),
                                        }
                                    });
                                },
                                "Save"
                            }
                        }
                    }
                }

                // TODO(human): Implement the sync trigger handler
                // This button should call bridge::invoke_trigger_sync() and display
                // the result (pulled/pushed counts) or error in sync_status.
                // Consider: should sync be disabled while in progress? What feedback
                // should the user see during a sync that takes a few seconds?
                button {
                    style: "
                        padding: 10px 20px;
                        background: #2d6a4f;
                        color: white;
                        border: none;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 14px;
                        font-weight: 500;
                    ",
                    onclick: move |_| {
                        sync_status.set(Some("Syncing...".into()));
                                    spawn(async move {
                                        match bridge::invoke_trigger_sync().await {
                                            Ok(synced_status) => {
                                                let SyncStatus{pulled, pushed} = synced_status;
                                                sync_status.set(Some(format!("Sync complete:\nItems pulled:{pulled}\nItems pushed:{pushed}")));
                                            }
                                            Err(e) => sync_status.set(Some(format!("Error: {e}"))),
                                        }
                                    });
                    },
                    "Sync Now"
                }

                // Status display
                if let Some(status) = &*sync_status.read() {
                    div {
                        style: "
                            margin-top: 12px;
                            padding: 8px 12px;
                            background: #f0f4f0;
                            border-radius: 6px;
                            font-size: 14px;
                            color: #333;
                        ",
                        "{status}"
                    }
                }
            }
        }
    }
}
