#!/usr/bin/env bash
#
# Run by ~/.dotfiles/scripts/session-work.sh immediately before the session-end
# auto-commit, and only because this file exists — repos that declare no presubmit
# pay a single stat() and are otherwise untouched.
#
# WHY THIS IS NOT JUST A NOTE IN CLAUDE.md. CI's `Formatting` job gates every push to
# main, and the auto-commit is authored by a shell script AFTER the model's last turn.
# A hand-run `cargo fmt` can therefore only ever cover edits made before that turn.
# That is not hypothetical: 509672c was a deliberate `[fmt]: rustfmt both workspaces`,
# and 8fce3fb — the auto-commit later the same day — swept 29 hunks of drift back in
# across core/auto_import/* and core/statement/*, and CI went red.
#
# TWO invocations, mirroring ci.yml's fmt job exactly. `tauri-app/frontend` is
# `exclude`d from the root workspace and declares its own bare `[workspace]`, so the
# `--all` above cannot reach it — verified: `cargo metadata` there reports the single
# package `frontend`. A subshell `cd` rather than `--manifest-path`, so this stays
# byte-identical to what CI checks.
#
# FMT ONLY — deliberately no clippy, no tests. Those are slow and cannot auto-fix, so a
# presubmit could only ever block on them, and a blocked auto-commit strands the
# session's work. The caller treats a non-zero exit as non-fatal for the same reason.
#
# Not `set -e`: the frontend must still be formatted if the root workspace fails (e.g.
# a parse error mid-edit), and the caller logs the exit code either way.
set -uo pipefail

cargo fmt --all
( cd tauri-app/frontend && cargo fmt --all )
