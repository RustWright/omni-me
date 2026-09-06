# NEXT

**Next action: stamp v1.1.0 and bump the private lock in the SAME stretch, then release the
client.** The server is already deployed (`sha-9a0b1dd`, health-gated OK 2026-09-06) and the
Phase C sync trap is gone — the box parses `record_type_declared`, so a Phase C client is safe
to install. Version is the user's call, already made: **1.1.0**; tags are immutable.

## ⚠️ The stamp and the private lock move together
`omni-me-private/Cargo.lock` pins the public crates by version and its Dockerfile builds
`--locked`: stamping alone breaks the **next** deploy, naming the lockfile not the release.
Re-resolve with `cargo metadata --format-version 1 >/dev/null` (3-line diff; not
`generate-lockfile`). Stamp only `Cargo.toml`, `tauri-app/frontend/Cargo.toml`,
`tauri.conf.json` — the `1.0.5` in `types.rs` and `feedback.rs` is a test fixture **and its
assertion**, which a sed rewrites in lockstep, leaving a green test that tests nothing. Then
tag, push, `gh workflow run 303775869 -f version=1.1.0 -f public_ref=v1.1.0 -f targets=both`.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY.** Real off switch: `feature.finances`.
- **`docs/src/invariants.md` is the contract** — never back-fit it. Phase C reads
  **(partly today)** deliberately: the declaration is live, but no screen edits it.
- **Empty required list ⇒ `complete = false`.** Fails closed; `complete` drives auto-close
  (⇒ read-only), plus `auto_close: false` on the minimal preset as a second guard.
- **Fresh install → minimal preset; existing → reflective.** The reflective fallback in
  `journal_record_type` is load-bearing: minimal would mark the back catalogue incomplete.
- **The overlay is a CI blind spot** — no push-triggered workflow, so it compiles against the
  engine only during a deploy. Cost one 17-min failed deploy 2026-09-06 (stale `origin/main`,
  6 unpushed commits). A `cargo check --locked -p omni-me-private` job is **agreed, pending**.

## Do NOT re-survey
Generalization is CLOSED — Phases 0, A, B, C built, the gate proven to bite. Don't re-derive the
feature map, the config foundation, or "UI stops at schema-driven forms" — see `invariants.md`.
**After the release, item 3 (AI/LLM/ML) is next; planning-first, give it a fresh session.**

## Open threads
⚠️ **`git status` the OVERLAY separately** — SessionEnd pushes only the current repo. · Held for
this release test pass: cross-device feedback capture, a real panic, two-device config override.
· ⚠️ `free -h` SWAP before heavy cargo (oomd killed the terminal scope 2026-09-06); disk 92%. ·
No UI for editing a declaration. · ⚠️ **Mock backend never persists** — autosaved edits revert
on navigation; mock limits, not bugs. · Finances still to be switched off on the device. ·
Curiosities→concepts + memory prune owed.
