#!/usr/bin/env bash
# Run the agent against an OpenRouter endpoint with the pin and the rate limit
# already set, and the key lifted out of credentials.toml without printing it.
#
#   scripts/bench-openrouter.sh --ask "What routines do I have?" --constrained
#   scripts/bench-openrouter.sh --bench
#
# Why a script rather than a documented `export` block: the key would otherwise
# have to appear on every invocation, and the two settings that are easiest to
# forget are the two that silently corrupt a result —
#
#   * an unpinned request lets the gateway route to any of a dozen upstreams at
#     differing quantizations, so the number describes "some stack". Constrained
#     decoding is a property of the serving stack, not the weights: one vendor's
#     config could not terminate at all with the same model another served in
#     1.3s. `--bench` cross-reads its two halves, so both must hit one stack.
#   * without a request interval a capped tier returns 429s that the scorecard
#     reports as the model failing.
#
# It always goes through `cargo run`. A bench takes 15+ minutes, long enough for
# the source to move underneath it, and a stale `target/debug` binary already
# produced one fabricated "-20 point constraint tax".
#
# Every setting below is overridable from the environment, because the next run
# of this is a different candidate model at Phase D.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
creds="${OMNI_CREDENTIALS:-$repo/secrets/credentials.toml}"

# `[openrouter] api_key`, which is where the Phase 0 spike put it. NOT `[llm]` —
# that section holds whatever endpoint the app itself is pointed at. The Rust
# side never sees `[openrouter]`: `credentials.rs` has no `deny_unknown_fields`,
# so the section is silently ignored there and has to be handed over explicitly.
if [[ -z "${OPENROUTER_KEY:-}" ]]; then
  OPENROUTER_KEY="$(python3 - "$creds" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    sys.exit(f"no credentials at {path}")
# Split on section headers rather than parsing TOML: 3.10 has no tomllib, and a
# dependency for one lookup is not worth it.
for chunk in re.split(r"^[ \t]*\[", path.read_text(), flags=re.M):
    if chunk.startswith("openrouter]"):
        m = re.search(r'^\s*api_key\s*=\s*"([^"]+)"', chunk, flags=re.M)
        if m:
            print(m.group(1), end="")
            break
else:
    sys.exit(f"no [openrouter] api_key in {path} — set $OPENROUTER_KEY instead")
PY
)"
fi
if [[ -z "$OPENROUTER_KEY" ]]; then
  echo "empty OpenRouter key" >&2
  exit 1
fi

# The model and its pin travel together: a tag is only valid for a model that
# provider actually serves, and `only` takes the full slug (with the quantization
# suffix) while `order` takes a bare one. Checked live against
# /api/v1/models/z-ai/glm-5.3-flash/endpoints — DeepInfra is the only
# cheapest-tier provider serving it with BOTH `tools` and `structured_outputs`,
# and it is one of two the design plan names as offering ZDR through OpenRouter.
model="${OMNI_BENCH_MODEL:-z-ai/glm-5.3-flash}"
pin="${OMNI_BENCH_PIN:-deepinfra/fp4}"

# Does the pinned endpoint actually offer what this invocation needs?
#
# `structured_outputs` is a property of the *tag*, not of the provider or the
# model: openai/gpt-oss-120b has it on deepinfra/bf16 and deepinfra/turbo and
# does NOT have it on deepinfra/fp8. Pinning the wrong one produces a
# constrained half that was never constrained — a clean-looking scorecard
# measuring nothing, which is the exact failure this whole harness keeps hitting.
#
# Only required when the run involves the constrained path; a plain `--ask` is
# fine on an endpoint that lacks it. And this is cheap prevention, not the
# guarantee — the agent counts replies that ignore the schema and withholds the
# tax when any do. So an unreachable API warns and continues; only a definite
# "this tag lacks it" aborts. Set OMNI_BENCH_SKIP_PREFLIGHT=1 to bypass.
needs_schema=no
for arg in "$@"; do
  case "$arg" in --bench | --constrained) needs_schema=yes ;; esac
done

if [[ "${OMNI_BENCH_SKIP_PREFLIGHT:-}" != "1" ]]; then
  python3 - "$model" "$pin" "$needs_schema" <<'PY' || exit 1
import json, sys, urllib.error, urllib.request

model, pin, needs_schema = sys.argv[1], sys.argv[2], sys.argv[3] == "yes"
url = f"https://openrouter.ai/api/v1/models/{model}/endpoints"
try:
    with urllib.request.urlopen(url, timeout=20) as response:
        data = json.load(response).get("data", {})
except Exception as e:  # unreachable, rate-limited, malformed — all the same here
    print(f"⚠️  could not pre-flight the pin ({e}); relying on the run's own "
          f"schema canary instead.", file=sys.stderr)
    sys.exit(0)

endpoints = {e.get("tag"): e.get("supported_parameters", []) or []
             for e in data.get("endpoints", [])}
if pin not in endpoints:
    sys.exit(f"pin `{pin}` is not an endpoint for {data.get('id', model)}.\n"
             f"  available: {', '.join(sorted(t for t in endpoints if t))}")

wanted = ["tools"] + (["structured_outputs"] if needs_schema else [])
missing = [w for w in wanted if w not in endpoints[pin]]
if missing:
    usable = sorted(t for t, params in endpoints.items()
                    if t and all(w in params for w in wanted))
    sys.exit(f"pin `{pin}` does not advertise {', '.join(missing)}, so this run "
             f"would measure nothing.\n  tags that do: {', '.join(usable) or '(none)'}")
PY
fi

export OMNI_AGENT_DATA="${OMNI_AGENT_DATA:-${TMPDIR:-/tmp}/omni-agent-bench}"
export OMNI_AGENT_LLM_BASE_URL="${OMNI_AGENT_LLM_BASE_URL:-https://openrouter.ai/api/v1}"
export OMNI_AGENT_LLM_MODEL="$model"
export OMNI_AGENT_LLM_API_KEY="$OPENROUTER_KEY"
export OMNI_AGENT_LLM_EXTRA_BODY="${OMNI_AGENT_LLM_EXTRA_BODY:-{\"provider\":{\"only\":[\"$pin\"],\"allow_fallbacks\":false}}}"
# Spacing only stops us *causing* a 429. The one seen live came from the pinned
# upstream's shared pool being overloaded by other traffic, which no interval can
# prevent — that is what the client's bounded retry is for.
export OMNI_AGENT_LLM_MIN_INTERVAL_MS="${OMNI_AGENT_LLM_MIN_INTERVAL_MS:-1000}"

# The model and the pin, never the key or the base URL — a URL can carry a key.
echo "model=$model pin=$pin data=$OMNI_AGENT_DATA" >&2
echo "⚠️  confirm every 'llm call' line reads provider=deepinfra; anything else" >&2
echo "   means the pin failed and the result is unattributable." >&2

cd "$repo"
exec cargo run -p omni-me-agent -- "$@"
