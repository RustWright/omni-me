//! Reading filed problem reports off the box.
//!
//! This is the whole retrieval half of feedback capture. The box holds every
//! report because every device pushes its events here, so one endpoint over the
//! tailnet serves all of them regardless of which device filed them.
//!
//! **Two representations, one query.** `?format=md` renders markdown for a
//! person (the default, because the reader is a session at a terminal); JSON is
//! there for anything that wants to route reports onward. Whatever calls this is
//! what makes the destination pluggable — a script writing into a private
//! overlay, or one opening issues — so the app needs no plugin seam of its own.
//!
//! **What this endpoint deliberately is not:** a triage API. There is no state
//! to mark a report done, because the acting-on happens in a repo and a commit,
//! not in the app.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

use omni_me_core::db::queries::{self, FeedbackReport};

use crate::AppState;

/// Cap on one response. High enough that a normal pull is a single request,
/// low enough that a runaway client cannot ask the box to render everything at
/// once. `since` is the intended way to page, since callers pull incrementally.
const DEFAULT_LIMIT: u32 = 200;
const MAX_LIMIT: u32 = 1000;

#[derive(Debug, Deserialize)]
pub struct FeedbackQuery {
    /// RFC3339 timestamp; only reports strictly newer are returned. A puller
    /// stores the newest timestamp it saw and passes it back next time.
    pub since: Option<String>,
    pub limit: Option<u32>,
    /// `md` (default) or `json`.
    pub format: Option<String>,
}

pub fn feedback_routes() -> Router<AppState> {
    Router::new().route("/feedback", get(list_handler))
}

async fn list_handler(
    State(state): State<AppState>,
    Query(q): Query<FeedbackQuery>,
) -> Result<Response, String> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let (reports, skipped) = queries::list_feedback(&state.db, q.since.as_deref(), limit)
        .await
        .map_err(|e| e.to_string())?;

    if q.format.as_deref() == Some("json") {
        return Ok(Json(reports).into_response());
    }

    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        render_markdown(&reports, skipped),
    )
        .into_response())
}

/// Render reports as markdown, newest first.
///
/// The skipped count is printed rather than dropped. A reader who sees "3
/// reports" cannot tell a quiet week from a decoder that silently ate half the
/// list; one that sees the count can.
fn render_markdown(reports: &[FeedbackReport], skipped: usize) -> String {
    let mut out = String::new();

    if reports.is_empty() && skipped == 0 {
        out.push_str("No feedback reports.\n");
        return out;
    }

    out.push_str(&format!("# Feedback — {} report(s)\n", reports.len()));
    if skipped > 0 {
        out.push_str(&format!(
            "\n> ⚠️ {skipped} report(s) could not be decoded and are not listed below.\n"
        ));
    }

    for r in reports {
        let p = &r.report;
        out.push_str(&format!("\n## {}\n\n", r.timestamp));
        out.push_str(&format!("{}\n\n", p.body.trim()));

        // Context as a definition list rather than a table: values here are
        // free-form text of unpredictable length (an editor buffer can be a
        // paragraph), and a markdown table with a long cell is unreadable in a
        // terminal.
        if let Some(screen) = &p.screen {
            match &p.screen_ref {
                Some(r) => out.push_str(&format!("- **Screen:** `{screen}` · `{r}`\n")),
                None => out.push_str(&format!("- **Screen:** `{screen}`\n")),
            }
        }
        let version = p.app_version.as_deref().unwrap_or("?");
        let platform = p.platform.as_deref().unwrap_or("?");
        out.push_str(&format!(
            "- **Build:** v{version} · {platform} · `{}`\n",
            r.device_id
        ));
        if p.non_production {
            match &p.data_dir {
                Some(dir) => out.push_str(&format!("- **⚠️ Sandbox run** — data root `{dir}`\n")),
                None => out.push_str("- **⚠️ Sandbox run** — not the live data root\n"),
            }
        }
        if let Some(url) = &p.server_url {
            out.push_str(&format!("- **Sync target:** `{url}`\n"));
        }
        if let Some(detail) = &p.screen_data {
            out.push_str(&format!("- **On screen:** {detail}\n"));
        }
        if !p.recent_errors.is_empty() {
            out.push_str("- **Errors:**\n");
            for e in &p.recent_errors {
                out.push_str(&format!("  - `{e}`\n"));
            }
        }
        if !p.recent_events.is_empty() {
            out.push_str("- **Recent events:**\n");
            for e in &p.recent_events {
                out.push_str(&format!("  - `{e}`\n"));
            }
        }
        // Both ids, always. The client mints `feedback_id` and the store assigns
        // the event id; keeping only one would make a divergence between them
        // invisible in the view people actually read, which is exactly the
        // symptom a duplicated or replayed append would produce.
        out.push_str(&format!(
            "- **Report id:** `{}` · **event:** `{}`\n",
            p.feedback_id, r.id
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use omni_me_core::events::FeedbackCapturedPayload;

    fn report(body: &str) -> FeedbackReport {
        FeedbackReport {
            id: "ev1".into(),
            device_id: "surface".into(),
            timestamp: "2026-09-05T12:00:00Z".into(),
            report: FeedbackCapturedPayload {
                feedback_id: "fb1".into(),
                body: body.into(),
                screen: Some("notes:edit".into()),
                app_version: Some("1.0.5".into()),
                platform: Some("linux".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn empty_list_says_so_rather_than_rendering_a_bare_heading() {
        assert_eq!(render_markdown(&[], 0), "No feedback reports.\n");
    }

    /// A decoder that quietly ate rows would make a partial list look complete.
    #[test]
    fn skipped_rows_are_announced() {
        let md = render_markdown(&[report("cursor jumped")], 2);
        assert!(md.contains("2 report(s) could not be decoded"));
    }

    #[test]
    fn report_renders_body_screen_and_build() {
        let md = render_markdown(&[report("cursor jumped")], 0);
        assert!(md.contains("cursor jumped"));
        assert!(md.contains("`notes:edit`"));
        assert!(md.contains("v1.0.5 · linux"));
        // Absent optional context must not render an empty bullet.
        assert!(!md.contains("**On screen:**"));
    }

    /// A sandbox report must name *which* data root. The flag alone says "a
    /// sandbox", and a test run is usually one of several.
    #[test]
    fn sandbox_runs_name_the_data_root() {
        let mut r = report("looked wrong");
        r.report.non_production = true;
        r.report.data_dir = Some("/tmp/omni-test-3".into());
        let md = render_markdown(&[r], 0);
        assert!(md.contains("Sandbox run"));
        assert!(md.contains("/tmp/omni-test-3"));
    }

    /// An older report predating `data_dir` still renders the flag rather than
    /// dropping the sandbox warning entirely.
    #[test]
    fn sandbox_without_a_data_root_still_warns() {
        let mut r = report("looked wrong");
        r.report.non_production = true;
        assert!(render_markdown(&[r], 0).contains("Sandbox run"));
    }

    /// Both ids render. The client mints one and the store assigns the other, so
    /// showing a single id would hide a divergence between them.
    #[test]
    fn both_the_report_id_and_the_event_id_render() {
        let md = render_markdown(&[report("cursor jumped")], 0);
        assert!(md.contains("`fb1`"), "missing client-minted id: {md}");
        assert!(md.contains("`ev1`"), "missing event id: {md}");
    }
}
