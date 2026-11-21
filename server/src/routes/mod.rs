mod auto_import;
mod blobs;
mod documents;
mod feedback;
mod jobs;
mod llm;
mod notes;
mod statements;
mod sync;
mod tasks;
mod wipe;

pub use auto_import::auto_import_routes;
pub use blobs::blob_routes;
pub use documents::documents_routes;
// The purge ticket is held on `AppState`, so its type has to be nameable there.
pub use documents::PurgeTicket;
pub use feedback::feedback_routes;
pub use jobs::jobs_routes;
pub use llm::llm_routes;
pub use notes::notes_routes;
pub use statements::statement_routes;
pub use sync::sync_routes;
pub use tasks::task_routes;
// Held on `AppState` for the same reason as `PurgeTicket`.
pub use wipe::{WipeTicket, wipe_routes};

#[cfg(test)]
mod tests {
    /// No handler may fail with a bare `String`: axum answers it `200 OK`, so the failure
    /// reads as success.
    /// Handlers are found by their `State<AppState>` extractor; helpers taking `&AppState` may
    /// keep `String`, and a handler with no state is not checked (none exists).
    /// One whitespace-stripped signature, ending at its opening brace.
    fn offends(sig: &str) -> bool {
        sig.contains("State<AppState>") && sig.ends_with(",String>{")
    }

    /// Every `async fn … {` in `text`, whitespace-stripped.
    fn signatures(text: &str) -> Vec<String> {
        // Whitespace stripped entirely rather than collapsed: rustfmt splits a
        // long signature across lines, and neither form survives a match against
        // the other. Same reason as the commands-crate guards.
        let flat: String = text.split_whitespace().collect();
        let mut out = Vec::new();
        let mut rest = flat.as_str();
        while let Some(at) = rest.find("asyncfn") {
            let after = &rest[at..];
            let Some(brace) = after.find('{') else { break };
            out.push(after[..=brace].to_string());
            rest = &after[brace..];
        }
        out
    }

    #[test]
    fn no_route_handler_fails_with_a_bare_string() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
        let mut offenders = Vec::new();

        for entry in std::fs::read_dir(&dir).expect("routes dir").flatten() {
            let path = entry.path();
            let is_rs = path.extension().and_then(|e| e.to_str()) == Some("rs");
            // This file declares modules and holds the guard; the only `async fn`
            // text in it is the broken signature quoted by the test below.
            let is_self = path.file_name().and_then(|f| f.to_str()) == Some("mod.rs");
            if !is_rs || is_self {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            for sig in signatures(&text) {
                if offends(&sig) {
                    offenders.push(format!("{name}: {}", &sig[..sig.len().min(60)]));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "these handlers report failure as HTTP 200 with a text/plain body — \
             give them `(StatusCode, String)` or a typed error: {offenders:#?}"
        );
    }

    /// A guard that cannot fire is worse than no guard, because it reads
    /// as coverage. These are the real signatures from either side of the fix:
    /// `list_handler` as it shipped broken, and as it stands now.
    #[test]
    fn the_guard_flags_the_signature_that_shipped_broken() {
        let broken = signatures(
            "async fn list_handler(
                State(state): State<AppState>,
                Query(q): Query<FeedbackQuery>,
            ) -> Result<Response, String> {",
        );
        assert_eq!(broken.len(), 1);
        assert!(offends(&broken[0]), "guard missed the historical defect");

        let fixed = signatures(
            "async fn list_handler(
                State(state): State<AppState>,
                Query(q): Query<FeedbackQuery>,
            ) -> Result<Response, (StatusCode, String)> {",
        );
        assert!(!offends(&fixed[0]), "guard rejects the corrected form");

        // A helper taking `&AppState` is not a handler and keeps `String`.
        let helper = signatures(
            "async fn store_blob(
                state: &AppState,
                body: &[u8],
            ) -> Result<AttachmentRef, String> {",
        );
        assert!(!offends(&helper[0]), "guard caught a non-handler helper");
    }
}
