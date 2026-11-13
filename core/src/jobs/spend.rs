//! What one run of a job spent on model calls.
//!
//! A run is metered by scoping its future with [`metered`]. Every model call made
//! inside that future reports its response here through [`record_response`], so a
//! job needs no plumbing of its own to account for spend.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Model spend over one run. `cost_micros` sums only the calls that reported a
/// cost, and `unpriced_calls` counts the rest, so an absent figure never reads as free.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spend {
    pub calls: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Millionths of a US dollar, from the provider's `usage.estimated_cost`.
    pub cost_micros: u64,
    pub unpriced_calls: u32,
}

impl Spend {
    /// Fold one chat-completions response body into the total.
    pub fn add_response(&mut self, response: &serde_json::Value) {
        let usage = &response["usage"];
        self.calls += 1;
        self.prompt_tokens += usage["prompt_tokens"].as_u64().unwrap_or(0);
        self.completion_tokens += usage["completion_tokens"].as_u64().unwrap_or(0);
        match usage["estimated_cost"].as_f64() {
            Some(dollars) if dollars >= 0.0 => {
                self.cost_micros += (dollars * 1_000_000.0).round() as u64;
            }
            _ => self.unpriced_calls += 1,
        }
    }
}

tokio::task_local! {
    static METER: Arc<Mutex<Spend>>;
}

/// Run `fut` with a fresh meter and return its output with what it spent.
///
/// The meter is task-local, so a model call moved onto a spawned task inside `fut`
/// is not counted. Mail and bank pulls are sequential, which is what this relies on.
pub async fn metered<F: std::future::Future>(fut: F) -> (F::Output, Spend) {
    let meter = Arc::new(Mutex::new(Spend::default()));
    let out = METER.scope(meter.clone(), fut).await;
    let spend = *meter.lock().unwrap_or_else(|e| e.into_inner());
    (out, spend)
}

/// Count a model response against the current run, if one is being metered.
pub fn record_response(response: &serde_json::Value) {
    let _ = METER.try_with(|m| {
        m.lock()
            .unwrap_or_else(|e| e.into_inner())
            .add_response(response)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn calls_inside_a_metered_run_are_summed() {
        let ((), spend) = metered(async {
            record_response(
                &json!({"usage": {"prompt_tokens": 100, "completion_tokens": 8,
                "estimated_cost": 0.000123}}),
            );
            record_response(&json!({"usage": {"prompt_tokens": 50, "completion_tokens": 2}}));
        })
        .await;
        assert_eq!(spend.calls, 2);
        assert_eq!(spend.prompt_tokens, 150);
        assert_eq!(spend.completion_tokens, 10);
        assert_eq!(spend.cost_micros, 123);
        assert_eq!(
            spend.unpriced_calls, 1,
            "a call with no cost is counted as unpriced"
        );
    }

    #[tokio::test]
    async fn a_call_outside_any_run_is_ignored() {
        record_response(&json!({"usage": {"estimated_cost": 1.0}}));
        let ((), spend) = metered(async {}).await;
        assert_eq!(spend, Spend::default());
    }
}
