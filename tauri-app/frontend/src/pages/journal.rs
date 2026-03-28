use dioxus::prelude::*;

use crate::components::editor::Editor;

#[component]
pub fn JournalPage() -> Element {
    rsx! {
        div {
            style: "max-width: 720px; margin: 0 auto;",

            h1 {
                style: "
                    font-size: 24px;
                    font-weight: 600;
                    margin: 0 0 16px 0;
                    color: #1a1a2e;
                ",
                "Journal"
            }

            Editor {
                initial_content: "# New Entry\n\nStart writing...".to_string(),
                on_change: move |content: String| {
                    web_sys::console::log_1(
                        &format!("Content changed: {} chars", content.len()).into(),
                    );
                },
            }
        }
    }
}
