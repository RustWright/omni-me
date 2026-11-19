use dioxus::prelude::*;

use crate::components::icon::{Icon, IconName};
use crate::money;

/// The header's eye: one tap hides every amount, another shows them.
#[component]
pub fn PrivacyEye() -> Element {
    let store = crate::continuity::use_continuity();
    let hidden = money::hidden();
    let (icon, label) = if hidden {
        (IconName::EyeOff, "Show amounts")
    } else {
        (IconName::Eye, "Hide amounts")
    };
    rsx! {
        button {
            class: if hidden {
                "p-1.5 rounded-md text-obsidian-accent hover:bg-obsidian-border/5 transition-colors"
            } else {
                "p-1.5 rounded-md text-obsidian-text-muted hover:text-obsidian-text hover:bg-obsidian-border/5 transition-colors"
            },
            "aria-label": "{label}",
            "aria-pressed": "{hidden}",
            title: "{label}",
            onclick: move |_| {
                money::toggle_hidden();
                store.persist_now();
            },
            Icon { name: icon, class: "w-5 h-5" }
        }
    }
}
