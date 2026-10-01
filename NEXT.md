# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — F5 onto the dev server, then verify on the dev phone
F2 + F2a are done and verified on the dev phone (1.1.18-dev); F5 is built and pushed (`3a5725e`).
`tasks.md` § F2 and § F5 carry both decision logs and what is still unverified. F5 needs a dev
image (`build-dev-image.yml`, `--ref dev/untested-overlay`), the dev container restarted on it, and
a `pdf_password_*` in the dev credentials. Both steps were refused by the permission classifier
as production reads, so they wait on him (below). Then: the boot log line
`textless pdfs re-read…`, and an encrypted statement opening on the phone.
Owed and not blocking: the Rust 1.99 upgrade (CI is pinned to 1.98.1), an APK carrying the F2 row
layout fix (`acace7c`), and the triage scoring query once batches carrying a verdict are reviewed.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **F5's two refused steps**: the dev image build/deploy, and deriving his bank's PDF password into
  the dev credentials (it reads the LIVE credentials file). Run them, or allow them.
- **Review the re-proposed September receipts** (~70 batches). F2/F2a make this workable now; his
  first commit with a correction is also the first real run of that path.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **Should a wipe clear the vector index synchronously?** `tasks.md` § DoR item 1, fourth finding.
- **The official finances import**: September has closed; when every statement is ready.
  ⛔ Gate 1, the LIVE server backup, comes first. Verify by size, never by exit code.
- **Mass document ingestion**: `archive::ingest_paths` has no caller yet.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
