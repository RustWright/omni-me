#!/usr/bin/env bash
# Run `bench-openrouter.sh --bench` across the whole candidate slate, keeping
# every scorecard on disk.
#
#   scripts/bench-slate.sh [output-dir]
#
# Why this exists rather than nine invocations by hand: the slate takes over two
# hours, and a scorecard that lives only in terminal scrollback is recoverable
# only from a session log — which is the exact complaint `bench-openrouter.sh`
# was written to fix for the shell *it* replaced. Nine models compared from
# memory is also how a stale number survives into a decision.
#
# ⚠️ **A seeded hub must already be running.** The cases name things that have to
# exist, or the run measures what a model does when the answer is absent:
#
#     mkdir hub && cd hub && cargo run -p omni-me-server     # one terminal
#     python3 scripts/seed-bench-hub.py                      # once
#
# One model failing must not end the run — `glm-5.3-flash` was rate-limited
# upstream on every provider tried on 2026-09-08, through no fault of ours. Each
# entry is therefore allowed to fail and the sweep continues; the summary at the
# end reports what is missing rather than pretending the slate is complete.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$repo/runs/$(date +%Y%m%d-%H%M%S)}"

# model | pin | arms
#
# Pins are resolved against OpenRouter's endpoint data and are **not
# interchangeable**: `openai/gpt-oss-120b` serves `structured_outputs` on bf16
# and turbo and NOT on fp8, so the wrong tag produces a constrained half that was
# never constrained. `bench-openrouter.sh` pre-flights this and aborts, so a
# stale row here fails loudly rather than quietly.
#
# The two `gpt-oss-120b` rows are deliberate and are not a duplicate: same
# weights, same gateway, different serving tier. They are the only pair on the
# slate where the sole variable is the stack, which is what makes them worth two
# slots — "is this model slow, or is this endpoint slow" has now been the wrong
# question three times in this project.
#
# `constrained-only` rows are not a limitation of the model. DeepInfra's Llama-4
# endpoints advertise `response_format` but not `tools`, so the free-form half
# cannot run there at all; the bench detects this and withholds the tax rather
# than reporting the constrained score as one.
SLATE=(
  "openai/gpt-oss-120b|deepinfra/bf16|both"
  "openai/gpt-oss-120b|deepinfra/turbo|both"
  "deepseek/deepseek-v4-flash|deepinfra/fp8|both"
  "qwen/qwen3.6-35b-a3b|deepinfra/fp8|both"
  "meta-llama/llama-4-scout|deepinfra/fp8|constrained-only"
  "meta-llama/llama-4-maverick|deepinfra/base|constrained-only"
  "z-ai/glm-5.3|deepinfra/fp4|both"
  "deepseek/deepseek-v4-pro|deepinfra/fp8|both"
  "qwen/qwen3.5-397b-a17b|deepinfra/fp8|both"
)

mkdir -p "$out"
echo "slate: ${#SLATE[@]} runs → $out" >&2

for row in "${SLATE[@]}"; do
  IFS='|' read -r model pin arms <<<"$row"
  # One file per model+pin, so the two gpt-oss rows cannot overwrite each other.
  slug="$(printf '%s__%s' "$model" "$pin" | tr '/' '-')"
  log="$out/$slug.log"

  echo "" >&2
  echo "─── $model @ $pin ($arms)" >&2
  # `--constrained` alongside `--bench` means "run the schema arm only". The
  # endpoint has no `tools` parameter, so the free-form arm does not exist there
  # and a both-arms run aborts in pre-flight instead of measuring anything.
  extra=()
  [[ "$arms" == "constrained-only" ]] && extra=(--constrained)
  if OMNI_BENCH_MODEL="$model" OMNI_BENCH_PIN="$pin" \
    "$repo/scripts/bench-openrouter.sh" --bench "${extra[@]}" >"$log" 2>&1; then
    echo "    done → $log" >&2
  else
    echo "    ⚠️ FAILED (exit $?) → $log — continuing" >&2
  fi
done

echo "" >&2
echo "═══ summary ═══" >&2
for row in "${SLATE[@]}"; do
  IFS='|' read -r model pin _ <<<"$row"
  slug="$(printf '%s__%s' "$model" "$pin" | tr '/' '-')"
  log="$out/$slug.log"
  printf '\n%s @ %s\n' "$model" "$pin"
  if [[ ! -s "$log" ]]; then
    echo "  (no output — the run did not start)"
    continue
  fi
  grep -E "free-form|schema-constrained|constraint tax|latency|tokens" "$log" || true

  # Every distinct upstream that served a call, because a pin that silently
  # failed makes every number above it unattributable.
  #
  # ⚠️ Match the value tolerantly, in TWO ways, each of which silently broke this
  # check when it was written naively:
  #   * `tracing`'s fmt layer renders string fields through `Debug`, so the line
  #     reads `provider="..."` WITH quotes.
  #   * the gateway reports the display name `DeepInfra`, not the routing tag
  #     `deepinfra` — the same word in two casings, one in the request and one in
  #     the response.
  # Either miss flags every clean run as unattributable, which is worse than no
  # check: it trains you to ignore the warning. Reporting *what was seen* rather
  # than a boolean also makes a real surprise legible instead of merely alarming.
  #
  # `unreported` counts as a miss here on purpose: this sweep goes through a
  # gateway, and a gateway that does not name its upstream has not evidenced the
  # pin. (A direct-to-provider run has no such field at all, which is why that
  # run is not driven from this script.)
  # ⚠️ Restricted to real `llm call` lines. Scanning the whole log matched
  # `bench-openrouter.sh`'s own advisory text — which contains the literal string
  # `provider=deepinfra;` — and the trailing semicolon then failed the exact
  # match, so a perfectly clean run reported itself as off-pin. A check that
  # reads its own documentation as evidence is worse than no check.
  others="$(grep -h 'llm call' "$log" 2>/dev/null |
    grep -oE 'provider="?[^",;[:space:]]+' |
    sed -E 's/^provider="?//' | sort -u | grep -ivx 'deepinfra' || true)"
  if [[ -n "$others" ]]; then
    echo "  ⚠️ NOT ALL CALLS WERE SERVED BY DEEPINFRA — saw: $(tr '\n' ' ' <<<"$others")"
    echo "     the numbers above describe a stack we do not ship"
  fi
done
