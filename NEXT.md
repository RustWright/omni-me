# NEXT

**Next action: verify v1.1.0 on the two devices, in the order below — then item 3 (AI/LLM/ML)
in a fresh, planning-first session.** Shipped 2026-09-06: server `sha-9a0b1dd` health-gated,
public tagged `v1.1.0` (full CI matrix green on `701b28d`), APK + AppImage published to
`/var/omni-updates` with both `latest.json` manifests written.

## Device verification — step 1 expires the moment you update
1. **Before updating anything**, confirm a still-v1.0.5 client syncs against the new server.
   This is what the deploy-first ordering existed to protect, and it is unrepeatable after.
2. OTA on the phone and on `surface`; each should report 1.1.0 after restart.
3. Open a daily journal entry — the reflection panel should draw from the declaration.
4. Legs held for this pass: cross-device feedback capture, a real panic reaching the
   diagnostic ring buffer, the two-device config override.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY.** Real off switch: `feature.finances`.
- **`docs/src/invariants.md` is the contract** — never back-fit it. Phase C reads
  **(partly today)** deliberately: the declaration is live, but no screen edits it.
- **Empty required list ⇒ `complete = false`.** Fails closed; `complete` drives auto-close
  (⇒ read-only), plus `auto_close: false` on the minimal preset as a second guard.
- **Fresh install → minimal preset; existing → reflective.** The reflective fallback in
  `journal_record_type` is load-bearing: minimal would mark the back catalogue incomplete.
- **A public version stamp and `omni-me-private/Cargo.lock` move together** — the overlay
  Dockerfile builds `--locked`, so stamping alone breaks the next deploy, naming the lockfile.
- **The overlay is a CI blind spot** — no push-triggered workflow, so it compiles against the
  engine only during a deploy. Cost one 17-min failed deploy 2026-09-06 (stale `origin/main`,
  6 unpushed commits). A `cargo check --locked -p omni-me-private` job is **agreed, pending**.

## Do NOT re-survey
Generalization is CLOSED — Phases 0, A, B, C built, the gate proven to bite. Don't re-derive the
feature map, the config foundation, or "UI stops at schema-driven forms" — see `invariants.md`.
**Item 3 (AI/LLM/ML) is planning-first by its own framing; give it a fresh session.**

## Open threads
⚠️ **`git status` the OVERLAY separately** — SessionEnd pushes only the current repo. · ⚠️
`free -h` SWAP before heavy cargo (oomd killed the terminal scope 2026-09-06); disk 92%. · No UI
for editing a declaration. · ⚠️ **Mock backend never persists** — autosaved edits revert on
navigation; mock limits, not bugs. · Finances still to be switched off on the device. ·
Curiosities→concepts + memory prune owed (Cycle 4 close-out).
