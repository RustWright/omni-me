//! Titles for citation chips, resolved from the device's own records.
//!
//! A citation carries kind and id only, so a renamed record still cites
//! correctly; the current title is looked up here. A chip that resolves nothing
//! shows its kind, since an id would be worse than useless.

use std::collections::HashMap;

use dioxus::prelude::*;

use crate::bridge;

pub type Titles = HashMap<(String, String), String>;

/// Current titles for `(kind, id)` pairs, empty until the lookup lands. Citations
/// never change after they are written, so this reads once per mount.
pub fn use_citation_titles(refs: Vec<(String, String)>) -> Titles {
    let titles = use_resource(move || {
        let refs = refs.clone();
        async move {
            if refs.is_empty() {
                return Titles::new();
            }
            match bridge::invoke_citation_titles(refs).await {
                Ok(rows) => rows
                    .into_iter()
                    .map(|(kind, id, title)| ((kind, id), title))
                    .collect(),
                Err(e) => {
                    web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(&format!(
                        "citation_titles failed, chips show kinds: {e}"
                    )));
                    Titles::new()
                }
            }
        }
    });
    titles.read().clone().unwrap_or_default()
}

/// The chip's text: the resolved title, then any title the wire carried, then the kind.
pub fn chip_label(titles: &Titles, kind: &str, id: &str, carried: Option<&String>) -> String {
    titles
        .get(&(kind.to_string(), id.to_string()))
        .or(carried)
        .cloned()
        .unwrap_or_else(|| kind.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resolved_title_wins_and_an_unresolved_chip_shows_its_kind() {
        let mut titles = Titles::new();
        titles.insert(("note".into(), "n1".into()), "Tax plan".into());
        assert_eq!(chip_label(&titles, "note", "n1", None), "Tax plan");
        assert_eq!(chip_label(&titles, "note", "n2", None), "note");
        let carried = "Old title".to_string();
        assert_eq!(
            chip_label(&titles, "note", "n2", Some(&carried)),
            "Old title"
        );
    }
}
