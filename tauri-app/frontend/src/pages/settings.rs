use dioxus::prelude::*;

#[component]
pub fn SettingsPage() -> Element {
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
                "Settings"
            }

            p {
                style: "color: #666; font-size: 16px;",
                "Settings will appear here"
            }
        }
    }
}
