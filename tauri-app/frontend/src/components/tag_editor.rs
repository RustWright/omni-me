//! Shared tag chip editor; the parent owns the tags, so only the draft is local. Tags pass
//! [`sanitize_tag`] by default, safe for inline `tags: [...]`. The archive must pass
//! `sanitize: false` (it strips the `:` of `key:value` tags), leaving `Tag::normalize` the only
//! authority; never add a second normalizer here.

use dioxus::prelude::*;

use crate::note_frontmatter::sanitize_tag;

/// A chip editor for a list of tags.
///
/// `on_add` gets a non-empty, non-duplicate tag; `on_remove` the chip's index; `sanitize`
/// defaults true, which every caller but the archive wants.
#[component]
pub fn TagChipEditor(
    tags: Vec<String>,
    #[props(default = false)] read_only: bool,
    #[props(default = true)] sanitize: bool,
    on_add: EventHandler<String>,
    on_remove: EventHandler<usize>,
) -> Element {
    let mut tag_draft = use_signal(String::new);
    // Snapshot the current tags for the dedup check inside the keydown handler.
    let existing = tags.clone();

    rsx! {
        div { class: "flex flex-wrap items-center gap-1.5 flex-1",
            for (idx , tag) in tags.iter().cloned().enumerate() {
                span {
                    key: "{idx}-{tag}",
                    class: "inline-flex items-center gap-1 px-2 py-0.5 bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20 rounded text-xs",
                    "#{tag}"
                    if !read_only {
                        button {
                            r#type: "button",
                            class: "text-obsidian-accent/70 hover:text-obsidian-accent leading-none",
                            "aria-label": "Remove tag",
                            onclick: move |_| on_remove.call(idx),
                            "×"
                        }
                    }
                }
            }
            if !read_only {
                input {
                    r#type: "text",
                    class: "flex-1 min-w-24 bg-transparent text-obsidian-text placeholder:text-obsidian-text-muted/50 focus:outline-none",
                    placeholder: "Add tag…",
                    value: "{tag_draft}",
                    oninput: move |e| tag_draft.set(e.value()),
                    onkeydown: move |e| {
                        if e.key().to_string() == "Enter" {
                            e.prevent_default();
                            let raw = tag_draft.peek().clone();
                            let t = if sanitize {
                                sanitize_tag(&raw)
                            } else {
                                raw.trim().to_lowercase()
                            };
                            if !t.is_empty() && !existing.contains(&t) {
                                on_add.call(t);
                            }
                            tag_draft.set(String::new());
                        }
                    },
                }
            }
        }
    }
}
