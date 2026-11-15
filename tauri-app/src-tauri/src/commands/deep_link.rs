//! `omni-me://<tab>` links, from a notification tap.
//!
//! The deep-link plugin (mobile only) hands over the URL at a cold start or while
//! running; `lib.rs` keeps the tab here and emits `deep-link:open`. The frontend
//! takes it at mount and on that event, and switches tab if the tab is shown.

use std::sync::Mutex;

use tauri::State;

/// The tab a link asked for, until the frontend takes it.
#[derive(Default)]
pub struct PendingDeepLink(pub Mutex<Option<String>>);

/// The tab named by an `omni-me://<tab>` URL, lowercased, or `None` for any
/// other URL. Which tabs exist is the frontend's to decide.
#[cfg_attr(not(mobile), allow(dead_code))]
pub fn tab_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("omni-me://")?;
    let tab: String = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    (!tab.is_empty() && tab.chars().all(|c| c.is_ascii_alphabetic())).then_some(tab)
}

impl PendingDeepLink {
    /// Only mobile builds receive links.
    #[cfg_attr(not(mobile), allow(dead_code))]
    pub fn offer(&self, url: &str) -> bool {
        match tab_of(url) {
            Some(tab) => {
                *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(tab);
                true
            }
            None => false,
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn take_deep_link_tab(pending: State<'_, PendingDeepLink>) -> Option<String> {
    pending.0.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_link_names_its_tab_and_nothing_else_does() {
        assert_eq!(tab_of("omni-me://finances").as_deref(), Some("finances"));
        assert_eq!(tab_of("omni-me://Settings/").as_deref(), Some("settings"));
        assert_eq!(tab_of("omni-me://archive?x=1").as_deref(), Some("archive"));
        assert_eq!(tab_of("omni-me://"), None);
        assert_eq!(tab_of("https://finances"), None);
        assert_eq!(tab_of("omni-me://../etc"), None);
    }

    #[test]
    fn taking_the_tab_empties_it() {
        let p = PendingDeepLink::default();
        assert!(p.offer("omni-me://finances"));
        assert_eq!(p.0.lock().unwrap().take().as_deref(), Some("finances"));
        assert!(p.0.lock().unwrap().is_none());
    }
}
