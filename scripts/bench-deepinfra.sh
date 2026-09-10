#!/usr/bin/env bash
# Run the agent against DeepInfra **direct** — the production path.
#
#   scripts/bench-deepinfra.sh --bench
#   scripts/bench-deepinfra.sh --ask "What routines do I have?"
#   OMNI_BENCH_SERVICE_TIER=priority scripts/bench-deepinfra.sh --bench
#
# The sibling of `bench-openrouter.sh`, which screens candidates through a
# gateway. Four things the gateway did for us do not exist here, and each one is
# a silent difference rather than a loud one — see MODEL_BENCH.md Part 3.
#
# ⚠️ **A seeded hub must already be running**, same as the gateway harness:
#
#     mkdir hub && cd hub && cargo run -p omni-me-server     # one terminal
#     python3 scripts/seed-bench-hub.py                      # once
#
# ⚠️ Always `cargo run`, never a prebuilt binary — a bench outlives its source,
# and a stale `target/debug` already produced one fabricated constraint tax.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
creds="${OMNI_CREDENTIALS:-$repo/secrets/credentials.toml}"

# `[llm]`, not a bench-only section. DeepInfra direct **is** production now, so
# the bench and the app read one endpoint and one key — that is the whole
# "a benchmark cannot validate a stack we do not ship" rule, taken to its end.
if [[ -z "${DEEPINFRA_KEY:-}" ]]; then
  DEEPINFRA_KEY="$(python3 - "$creds" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    sys.exit(f"no credentials at {path}")
# Split on section headers rather than parsing TOML: 3.10 has no tomllib.
for chunk in re.split(r"^[ \t]*\[", path.read_text(), flags=re.M):
    if chunk.startswith("llm]"):
        m = re.search(r'^\s*api_key\s*=\s*"([^"]+)"', chunk, flags=re.M)
        if m:
            print(m.group(1), end="")
            break
else:
    sys.exit(f"no [llm] api_key in {path} — set $DEEPINFRA_KEY instead")
PY
)"
fi
[[ -n "$DEEPINFRA_KEY" ]] || { echo "empty DeepInfra key" >&2; exit 1; }

base_url="${OMNI_BENCH_BASE_URL:-https://api.deepinfra.com/v1/openai}"
# The role-A leading candidate. ⚠️ DeepInfra's spelling, which is NOT
# OpenRouter's: `deepseek/deepseek-v4-flash` there is `deepseek-ai/DeepSeek-V4-Flash`
# here, case-sensitive. Every pin in `bench-slate.sh` is in the gateway's dialect
# and must be re-resolved before it is used against this script.
model="${OMNI_BENCH_MODEL:-openai/gpt-oss-120b}"

# Existence, not capability. DeepInfra publishes no machine-readable
# `supported_parameters`, so the gateway pre-flight (which aborts a constrained
# run on an endpoint lacking `structured_outputs`) has no equivalent and the
# run's own `off_schema` canary is the only backstop left. What this *can* still
# catch is a misspelled id — cheap, and the most likely mistake on this path,
# since the tier is part of the id here (`…-Turbo`, `…-Ultra`) rather than a pin.
#
# ⚠️ This listing is PUBLIC — it answers 200 with no credentials — so a pass says
# nothing about whether the key is valid. Only a real call proves that.
# Advisory only: an unreachable listing must not block a bench.
if [[ "${OMNI_BENCH_SKIP_PREFLIGHT:-}" != "1" ]]; then
  python3 - "$model" "$base_url" "$DEEPINFRA_KEY" <<'PY' || exit 1
import json, sys, urllib.error, urllib.request

model, base_url, key = sys.argv[1], sys.argv[2].rstrip("/"), sys.argv[3]
req = urllib.request.Request(f"{base_url}/models",
                             headers={"Authorization": f"Bearer {key}"})
try:
    with urllib.request.urlopen(req, timeout=20) as response:
        ids = {m.get("id") for m in json.load(response).get("data", [])}
except Exception as e:
    print(f"⚠️  could not list models ({e}); skipping the id check.", file=sys.stderr)
    sys.exit(0)
if model in ids:
    sys.exit(0)
# Case-only misses are the expected failure, so name the near match rather than
# printing a catalogue of hundreds.
near = [i for i in ids if i and i.lower() == model.lower()]
sys.exit(f"`{model}` is not in DeepInfra's catalogue."
         + (f"\n  did you mean `{near[0]}`?" if near else
            "\n  check the spelling on the model's page at deepinfra.com"))
PY
fi

export OMNI_AGENT_DATA="${OMNI_AGENT_DATA:-${TMPDIR:-/tmp}/omni-agent-bench-direct}"
export OMNI_AGENT_LLM_BASE_URL="$base_url"
export OMNI_AGENT_LLM_MODEL="$model"
export OMNI_AGENT_LLM_API_KEY="$DEEPINFRA_KEY"

# ⚠️ NO `extra_body`. `provider.only` / `zdr` / `data_collection` /
# `require_parameters` are OpenRouter's request vocabulary and mean nothing here.
# `require_parameters` is the one worth mourning: it turned "this endpoint
# ignores response_format" into a routing ERROR, and direct there is no such
# signal — an ignored parameter is simply ignored. Zero-retention is an ACCOUNT
# setting on this path, not a per-request assertion.
#
# The one top-level key worth sending is the latency lever: DeepInfra's
# `service_tier` is the direct analogue of pinning a fast serving tier, since the
# quantization tags we pinned through the gateway are not selectable here.
# Billed at 1.5× only when priority is actually delivered, and the response
# echoes back which tier served it.
if [[ -n "${OMNI_BENCH_SERVICE_TIER:-}" ]]; then
  export OMNI_AGENT_LLM_EXTRA_BODY="{\"service_tier\":\"$OMNI_BENCH_SERVICE_TIER\"}"
fi

# ⚠️ Unset by default, unlike the gateway harness. There, 1s spacing guarded a
# SHARED upstream pool whose 429s came from other people's traffic. DeepInfra
# caps *concurrency* (200 simultaneous requests per model) and the agent issues
# one at a time, so spacing here would cap us at 1 req/s for nothing. The
# client's bounded backoff still covers the 429s the docs warn about while
# capacity autoscales. Set OMNI_AGENT_LLM_MIN_INTERVAL_MS in the environment to
# reintroduce spacing deliberately; nothing here sets it.

echo "model=$model base=$base_url data=$OMNI_AGENT_DATA" >&2
# ⚠️ `provider=` is an OpenRouter field. Direct, it is absent and logs as `None`
# — expected, and it means the pin-attribution check in `bench-slate.sh` proves
# nothing here. `service_tier` in the response is the attribution channel now.
echo "note: 'llm call' lines will show no provider= — that is correct direct." >&2

cd "$repo"
exec cargo run -p omni-me-agent -- "$@"
