//! Shared tag chip-editor used by both the journal and generic-note properties
//! panels. Renders the current tags as removable chips plus an
//! "Add tag…" input; the parent owns the tag vector and mutates it in the
//! `on_add` / `on_remove` handlers, so this component stays stateless except for
//! the in-progress draft.
//!
//! Tags are run through [`sanitize_tag`] here, so every emitted tag is safe for
//! the inline `tags: [...]` serialization (never needs quoting).

use dioxus::prelude::*;

use crate::note_frontmatter::sanitize_tag;

/// A chip editor for a list of tags.
///
/// - `tags` — the current tags (owned by the parent; re-passed each render).
/// - `on_add` — fired with an already-sanitized, non-empty, non-duplicate tag.
/// - `on_remove` — fired with the index of the chip to remove.
#[component]
pub fn TagChipEditor(
    tags: Vec<String>,
    #[props(default = false)] read_only: bool,
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
                            let t = sanitize_tag(&tag_draft.peek());
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
