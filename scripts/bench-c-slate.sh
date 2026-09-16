#!/usr/bin/env bash
# Run one role-C bench across the vision slate, keeping every scorecard on disk.
#
#   scripts/bench-c-slate.sh transcription [out-dir] [model ...]
#   scripts/bench-c-slate.sh extraction    runs/c1 Qwen/Qwen3-VL-30B-A3B-Instruct
#
# The bench is an argument rather than a loop over all three because the slate is
# staged: transcription first across everything, then only its survivors through
# extraction and reading. C3's oracle is a born-digital PDF's own text layer —
# exact, free, and unambiguous — so it is the cheapest honest way to find the
# models that cannot read a rendered page at all. Naming models explicitly is how
# the later rounds are restricted to those survivors.
#
# ⚠️ **Direct to DeepInfra, and that is forced rather than preferred.**
# `build_extractor`, `build_reader` and `build_transcriber` take only
# `&Credentials` — no `ClientOptions` — so `OMNI_AGENT_LLM_EXTRA_BODY` cannot
# reach them and a gateway run could not carry a provider pin. Unpinned, the
# gateway routes to any upstream at any quantization and the number describes
# "some stack". Direct is one stack by construction. This is R6's sibling: the
# pre-flight guard does not port to direct, and the pin does not port to role C.
#
# ⚠️ **The model is set through a generated credentials file, never through
# `OMNI_AGENT_LLM_MODEL`.** Any of `OMNI_AGENT_LLM_{BASE_URL,MODEL,API_KEY}`
# trips the override branch in `agent/src/main.rs`, which hardcodes
# `vision: false` and clears `[llm.extractor]` — after which every role-C builder
# refuses and the run scores nothing while looking configured.
#
# ⚠️ **No request spacing reaches role C.** `min_interval` is also a
# `ClientOptions` field, so a rate-capped endpoint surfaces as the model failing.
# A 429-heavy scorecard here is a property of the tier, not of the weights;
# `MODEL_BENCH.md` records `glm-5.3-flash` as the standing example.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
creds="${OMNI_CREDENTIALS:-$repo/secrets/credentials.toml}"

bench="${1:-}"
case "$bench" in
  transcription|extraction|reading) ;;
  *)
    echo "usage: $0 <transcription|extraction|reading> [out-dir] [model ...]" >&2
    exit 2
    ;;
esac
shift

out="${1:-$repo/runs/$(date +%Y%m%d-%H%M%S)-c-$bench}"
[ $# -gt 0 ] && shift

# The slate, cheapest first. Ids are DeepInfra's own spelling and are
# case-sensitive on both halves — re-resolved against the live catalogue
# 2026-09-15. Every vendor here clears `refusal_reason`'s open-weights check;
# `google/gemma*` passes through the mixed-vendor rule, and `google/gemini*`
# would not.
SLATE=(
  "inclusionAI/Ling-3.0-flash-VL"
  "google/gemma-4-26B-A4B-it"
  "mistralai/Mistral-Small-3.2-24B-Instruct-2506"
  "google/gemma-3-27b-it"
  "Qwen/Qwen3.5-9B"
  "meta-llama/Llama-4-Scout-17B-16E-Instruct"
  "Qwen/Qwen3.6-35B-A3B"
  "google/gemma-4-31B-it"
  "zai-org/GLM-5.3-Flash"
  "Qwen/Qwen3-VL-30B-A3B-Instruct"
  "deepseek-ai/DeepSeek-V4.1-Flash"
  "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"
  "Qwen/Qwen3-VL-235B-A22B-Instruct"
  "deepseek-ai/DeepSeek-V4-Flash-Vision-Exp"
)
# Any models named on the command line replace the slate, which is how the
# post-transcription rounds are held to the survivors.
[ $# -gt 0 ] && SLATE=("$@")

# The DeepInfra key, lifted out of `[llm]` without printing it. `[openrouter]`
# is the wrong section here: this run does not go through the gateway.
KEY="$(python3 - "$creds" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
for chunk in re.split(r'^\[', text, flags=re.M):
    if chunk.startswith('llm]'):
        m = re.search(r'^\s*api_key\s*=\s*"([^"]+)"', chunk, flags=re.M)
        if m:
            print(m.group(1))
        break
PY
)"
if [ -z "$KEY" ]; then
  echo "no [llm] api_key in $creds" >&2
  exit 1
fi

mkdir -p "$out"
echo "slate: ${#SLATE[@]} × --bench-$bench → $out" >&2

# 0600 from creation, not chmod-ed afterwards: a world-readable instant is still
# an exposure, and this file carries the key in cleartext.
umask 077
tmpcreds="$(mktemp)"
trap 'rm -f "$tmpcreds"' EXIT

missing=()
for model in "${SLATE[@]}"; do
  tag="$(echo "$model" | tr '/' '-')"
  log="$out/$tag.log"
  echo "" >&2
  echo "─── $model" >&2

  cat > "$tmpcreds" <<TOML
[llm]
provider = "openai_compatible"
base_url = "https://api.deepinfra.com/v1/openai"
model = "$model"
api_key = "$KEY"
vision = true
allow_closed_weights = false
TOML

  # One failure must not end the slate. An endpoint can be rate-limited or
  # withdrawn upstream through no fault of ours, and losing the other thirteen
  # scorecards to it is how a sweep gets run twice.
  if OMNI_AGENT_CREDENTIALS="$tmpcreds" \
     OMNI_AGENT_DATA="${OMNI_AGENT_DATA:-${TMPDIR:-/tmp}/omni-agent-c-bench}" \
     cargo run -p omni-me-agent -- "--bench-$bench" > "$log" 2>&1; then
    tail -n 12 "$log" >&2
  else
    echo "  FAILED — see $log" >&2
    missing+=("$model")
  fi
done

echo "" >&2
echo "=== slate complete: $out" >&2
if [ ${#missing[@]} -gt 0 ]; then
  # Named, not counted. "11 of 14 succeeded" does not say which three to look at,
  # and a slate that reports only a total reads as complete.
  echo "=== ${#missing[@]} did not produce a scorecard:" >&2
  printf '      %s\n' "${missing[@]}" >&2
fi
