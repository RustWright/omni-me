#!/usr/bin/env bash
# Run `--bench-structuring` across the text slate, keeping every scorecard.
#
#   scripts/bench-d-slate.sh [out-dir] [model ...]
#
# ⚠️ **This one goes through the gateway, unlike `bench-c-slate.sh`** — and the
# reason is worth stating, because the two look like they should be symmetric.
# `bench-openrouter.sh` sets `OMNI_AGENT_LLM_*`, which trips `load_credentials`'
# override branch: it hardcodes `vision: false` and clears every per-role table.
# That is fatal for role C, whose builders refuse without vision. Role D is
# text-only, so `vision: false` costs nothing, and `for_role` falls back to the
# base `[llm]` the override just populated. Going through the gateway therefore
# buys D the whole `bench-slate.sh` apparatus for free: verified slugs, a
# provider pin, `min_interval` spacing, and the pre-flight that aborts when a
# tag does not serve the parameter the arm needs.
#
# ⚠️ **The two `llama-4` rows from the A/B slate are deliberately absent.** Their
# endpoints offer `response_format` and no `tools` parameter, and seat D's first
# hard gate is *supports tool calls at all* — so they are excluded by the
# threshold rather than skipped for convenience. Naming a model on the command
# line overrides the slate if you want to confirm that.
#
# ⚠️ Repeats default to 3 (`OMNI_BENCH_STRUCT_REPEATS`), and this arm was built
# that way from the start. ⛔ Do not lower it: R26 showed a single sample cannot
# rank a seat, and D's `AGREE` column is the thing that makes the variance
# visible rather than silent.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$repo/runs/$(date +%Y%m%d-%H%M%S)-d}"
[ $# -gt 0 ] && shift

# model|pin — the seven rows of the A/B slate that serve `tools`. Pins are not
# interchangeable and are the same ones `bench-slate.sh` resolved; a stale tag
# fails loudly in pre-flight rather than quietly measuring another stack.
SLATE=(
  "openai/gpt-oss-120b|deepinfra/bf16"
  "openai/gpt-oss-120b|deepinfra/turbo"
  "deepseek/deepseek-v4-flash|deepinfra/fp8"
  "deepseek/deepseek-v4-pro|deepinfra/fp8"
  "qwen/qwen3.6-35b-a3b|deepinfra/fp8"
  "qwen/qwen3.5-397b-a17b|deepinfra/fp8"
  "z-ai/glm-5.3|deepinfra/fp4"
)
[ $# -gt 0 ] && SLATE=("$@")

mkdir -p "$out"
echo "slate: ${#SLATE[@]} × --bench-structuring → $out" >&2

missing=()
for row in "${SLATE[@]}"; do
  IFS='|' read -r model pin <<<"$row"
  # One file per model+pin: the two gpt-oss rows are the same weights on
  # different serving tiers and must not overwrite each other.
  slug="$(printf '%s__%s' "$model" "${pin:-none}" | tr '/' '-')"
  log="$out/$slug.log"

  echo "" >&2
  echo "─── $model @ ${pin:-unpinned}" >&2
  # One failure must not end the slate, for `bench-slate.sh`'s reason: an
  # endpoint can be rate-limited upstream through no fault of ours, and losing
  # the other six scorecards to it is how a sweep gets run twice.
  if OMNI_BENCH_MODEL="$model" OMNI_BENCH_PIN="$pin" \
     OMNI_AGENT_DATA="${OMNI_AGENT_DATA:-${TMPDIR:-/tmp}/omni-agent-d-bench}" \
     "$repo/scripts/bench-openrouter.sh" --bench-structuring >"$log" 2>&1; then
    tail -n 14 "$log" >&2
  else
    echo "  FAILED (exit $?) — see $log" >&2
    missing+=("$model @ $pin")
  fi
done

echo "" >&2
echo "=== slate complete: $out" >&2

# Every distinct upstream that served a call. A pin that silently failed makes
# every number above it unattributable, and `tracing` renders the field through
# `Debug`, so the quotes are real and the gateway answers `DeepInfra` where the
# request said `deepinfra`. Restricted to `llm call` lines: scanning the whole
# log matches this script's own prose.
for row in "${SLATE[@]}"; do
  IFS='|' read -r model pin <<<"$row"
  slug="$(printf '%s__%s' "$model" "${pin:-none}" | tr '/' '-')"
  log="$out/$slug.log"
  [ -s "$log" ] || continue
  others="$(grep -h 'llm call' "$log" 2>/dev/null |
    grep -oE 'provider="?[^",;[:space:]]+' |
    sed -E 's/^provider="?//' | sort -u | grep -ivx 'deepinfra' || true)"
  if [ -n "$others" ]; then
    echo "  ⚠️ $model @ $pin — NOT ALL CALLS SERVED BY DEEPINFRA: $(tr '\n' ' ' <<<"$others")" >&2
  fi
done

if [ ${#missing[@]} -gt 0 ]; then
  # Named, not counted: "5 of 7 succeeded" does not say which two to look at.
  echo "=== ${#missing[@]} did not produce a scorecard:" >&2
  printf '      %s\n' "${missing[@]}" >&2
fi
