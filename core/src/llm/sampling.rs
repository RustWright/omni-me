//! What a request says about how the model should sample, and why role C is
//! sampled differently from the chat seats: `docs/src/assistant.md` § How each
//! seat is sampled.

use serde_json::{Value, json};

/// The sampling parameters a client sends with every request.
///
/// `None` does not send the key at all, which is not the same as sending a provider's
/// documented default — an endpoint is free to change that default under us, and
/// then the same request means something new without anything here changing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sampling {
    /// 0.0 asks for the most likely token every time, as far as the stack allows.
    ///
    /// `f64` because both ends of the journey are: a TOML float parses as one, and
    /// a JSON number is one. Declared `f32`, a configured `0.9` reaches the wire as
    /// `0.8999999761581421` — parseable, unequal, and invisible in a diff.
    pub temperature: Option<f64>,
    /// Nucleus cutoff. Left unset wherever `temperature` is set; the docs section
    /// says why sending both is worse than sending either.
    pub top_p: Option<f64>,
    /// A reproducibility request, honoured by some serving stacks and ignored by
    /// others. Never a guarantee — see the docs section.
    pub seed: Option<u64>,
}

impl Sampling {
    /// Send nothing, which is what every request in this codebase did until
    /// 2026-09-27. Named rather than spelled `default()` at call sites so a seat
    /// can ask for it deliberately.
    pub const fn provider_default() -> Self {
        Self {
            temperature: None,
            top_p: None,
            seed: None,
        }
    }

    /// Temperature 0 and nothing else, for a seat whose question has one right
    /// answer.
    ///
    /// No `seed`, and that is a trap rather than an omission: a gateway told to
    /// route only to endpoints supporting every parameter we send will refuse the
    /// ones without `seed`, turning a measurement into a routing error on some
    /// rows and silently shrinking the slate. `scripts/bench-openrouter.sh`
    /// reached the same conclusion from the other end.
    pub const fn deterministic() -> Self {
        Self {
            temperature: Some(0.0),
            top_p: None,
            seed: None,
        }
    }

    /// Whether anything at all is sent.
    pub fn is_provider_default(&self) -> bool {
        *self == Self::provider_default()
    }

    /// Insert every set parameter into a request body, leaving the rest alone.
    ///
    /// Applied before `extra_body` at both call sites, so a caller that names a
    /// parameter explicitly still wins — the precedence `min_interval` already has.
    pub fn apply_to(&self, body: &mut Value) {
        let Some(target) = body.as_object_mut() else {
            return;
        };
        if let Some(t) = self.temperature {
            target.insert("temperature".to_string(), json!(t));
        }
        if let Some(p) = self.top_p {
            target.insert("top_p".to_string(), json!(p));
        }
        if let Some(s) = self.seed {
            target.insert("seed".to_string(), json!(s));
        }
    }

    /// Fold a caller's `extra_body` over this, so a description can be honest.
    ///
    /// A bench harness sets sampling through the same object it sends its gateway
    /// pin in, and that wins at the client. A header printed from the role default
    /// alone would name a temperature the request did not carry.
    pub fn with_overrides(mut self, extra: Option<&Value>) -> Self {
        let Some(Value::Object(extra)) = extra else {
            return self;
        };
        if let Some(t) = extra.get("temperature").and_then(Value::as_f64) {
            self.temperature = Some(t);
        }
        if let Some(p) = extra.get("top_p").and_then(Value::as_f64) {
            self.top_p = Some(p);
        }
        if let Some(s) = extra.get("seed").and_then(Value::as_u64) {
            self.seed = Some(s);
        }
        self
    }

    /// One line for a log or a bench header: what this run actually sent.
    ///
    /// A scorecard that does not carry this cannot be compared against the next
    /// one, which is the whole finding behind `MODEL_BENCH.md` R26.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(t) = self.temperature {
            parts.push(format!("temperature={t}"));
        }
        if let Some(p) = self.top_p {
            parts.push(format!("top_p={p}"));
        }
        if let Some(s) = self.seed {
            parts.push(format!("seed={s}"));
        }
        if parts.is_empty() {
            return "provider default (nothing sent)".to_string();
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_parameter_is_not_sent() {
        // The distinction the whole type exists for: absent is not zero, and a
        // body carrying `"temperature": null` would be a third thing again.
        let mut body = json!({ "model": "m" });
        Sampling::provider_default().apply_to(&mut body);
        assert_eq!(body, json!({ "model": "m" }));
    }

    #[test]
    fn deterministic_sends_temperature_and_withholds_seed() {
        let mut body = json!({ "model": "m" });
        Sampling::deterministic().apply_to(&mut body);
        assert_eq!(body["temperature"], json!(0.0));
        assert!(
            body.get("seed").is_none(),
            "a seed sent under require_parameters turns an unsupporting endpoint \
             into a routing error; see Sampling::deterministic"
        );
        assert!(body.get("top_p").is_none());
    }

    #[test]
    fn apply_to_leaves_the_rest_of_the_body_alone() {
        let mut body = json!({ "model": "m", "messages": [], "max_tokens": 10 });
        Sampling {
            temperature: Some(0.2),
            top_p: Some(0.9),
            seed: Some(7),
        }
        .apply_to(&mut body);
        assert_eq!(body["max_tokens"], json!(10));
        assert_eq!(body["messages"], json!([]));
        assert_eq!(body["top_p"], json!(0.9));
        assert_eq!(body["seed"], json!(7));
    }

    #[test]
    fn an_extra_body_temperature_is_what_a_header_should_name() {
        // `bench-openrouter.sh` sends temperature inside the same object as the
        // gateway pin, and that reaches the wire. A header printed from the role
        // default alone would name 0 on a run that sampled at 0.4.
        let folded = Sampling::deterministic().with_overrides(Some(
            &json!({ "provider": { "only": ["x"] }, "temperature": 0.4 }),
        ));
        assert_eq!(folded.temperature, Some(0.4));
        assert_eq!(folded.describe(), "temperature=0.4");
    }

    #[test]
    fn an_extra_body_that_names_no_sampling_changes_nothing() {
        let pin_only = Sampling::deterministic()
            .with_overrides(Some(&json!({ "provider": { "only": ["x"] } })));
        assert_eq!(pin_only, Sampling::deterministic());
        assert_eq!(
            Sampling::deterministic().with_overrides(None),
            Sampling::deterministic()
        );
    }

    #[test]
    fn the_description_says_when_nothing_is_sent() {
        // Printed into every bench header, so "" would read as "not instrumented"
        // rather than as the fact it is.
        assert_eq!(
            Sampling::provider_default().describe(),
            "provider default (nothing sent)"
        );
        assert_eq!(Sampling::deterministic().describe(), "temperature=0");
        assert_eq!(
            Sampling {
                temperature: Some(0.7),
                top_p: None,
                seed: Some(1),
            }
            .describe(),
            "temperature=0.7 seed=1"
        );
    }
}
