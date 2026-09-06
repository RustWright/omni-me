# NEXT

**Next action: deploy the server, THEN install a client — see the ordering trap below.**
Phase C — record types — is **built, green and UI-verified**: core 632, frontend 115, clippy
clean in both wasm configs, Playwright at 390/1280 with **zero** console errors, and the panel
confirmed drawing from the declaration.

## ⚠️ Release ordering — do this before installing any client
`sync.rs::push_handler` parses **every** event in a batch before appending **any**, so an
unknown `event_type` 400s the whole batch. Phase C adds `record_type_declared`: a client
emitting it against the deployed server breaks **all** sync, journal edits included. **Deploy
first**: public push → `gh workflow run 303640807 -f public_ref=main` (~18 min, health-gated).
The server needs no projection — it runs zero — only a core build that can parse the type.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY.** Real off switch: `feature.finances`.
- **`docs/src/invariants.md` is the contract** — never back-fit it. Phase C reads
  **(partly today)** deliberately: the declaration is live, but no screen edits it.
- **Properties are prose boxes only.** The payload carries `kind` with `text` its only value —
  the log is permanent, so the field ships now; new kinds are a value change later.
- **Empty required list ⇒ `complete = false`, never true.** `[].iter().all()` is `true`, and
  `complete` drives auto-close (⇒ read-only). Fails closed, plus `auto_close: false` on the
  minimal preset as a second guard.
- **Fresh install → minimal preset; existing → reflective**, by whether the log holds journal
  events. The reflective fallback in `journal_record_type` is load-bearing: minimal would mark
  the whole back catalogue incomplete.

## Do NOT re-survey
Generalization is CLOSED — Phases 0, A, B, C all built; the gate was run **and proven to bite**
(sabotaging the fallback made it fail). Don't re-derive the feature map, the config foundation,
or "UI stops at schema-driven forms" — that is in `invariants.md`. **After the deploy, item 3
(AI/LLM/ML) is next, and it is planning-first by its own framing — give it a fresh session.**

## Open threads
⚠️ **Check `free -h` SWAP before heavy cargo** — `systemd-oomd` killed the whole terminal scope
2026-09-06 at swap 1.6/1.9GB; diagnosis in memory `project_release_builds`. · Disk 92%. · No UI
for editing a declaration yet. · ⚠️ **The mock backend never persists**, so an *autosaved* edit
reverts on navigation and the mock entry has no frontmatter — mock limits, not bugs; don't
re-diagnose them. · Finances still to be switched off on the user's device. ·
Curiosities→concepts + memory prune owed. · `npm run copy:editor:dev` after every dx rebuild.
