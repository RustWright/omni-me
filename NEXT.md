# NEXT

✅ **WSL2 box: toolchain in** — rust 1.98.1+wasm32, dx 0.7.2, cargo-tauri, mdbook, node 20 (nvm),
`.ort/`, `node_modules/`, **privacy guard installed (self-test PASSED)**; it and the denylist were
absent while SessionEnd auto-pushes this repo. Branches fixed. Rest: overlay `HANDOFF.md` § 3c.

## ▶ NEXT ACTION — two independent threads, either order

**A. The box is up** — apt, tailnet (`desktop-9m2omog`), box health ok, `galaxy-s9` authorized
(API 29, right phone). ⛔ Two things left: `wsl --shutdown` to apply 6GB/8GB (⚠️ **the first
`cargo check` should wait for it** — 8 CPUs on 3.8 GiB OOMs), then that check as the end-to-end
proof. ⚠️ adb re-auth traps are in § 3c; neither is a device fault.

**B. The widen — your hands, unchanged.** ⛔ Blocked for me: the classifier refuses the append.
`sudo tail -n +31 /etc/omni-me/credentials.toml | sudo tee -a /etc/omni-me/credentials-dev.toml > /dev/null`
then `sudo docker restart omni-me-dev`. ⛔ The append IS the scope control — no per-account flag.
Backups: `/var/omni-snapshots/dev-pre-widen-20260921-0400.tgz` + credentials copy. Then dev deploy,
read a tick (`effective_confidence`, `needs_manual_review`, `dropped_postings`, `warnings`), then
the APK — warnings on the draft form, `x-filename` from the picker not synthesized in Rust.

## ✅ Decided
- **Salvage over strictness** (2026-09-21): drop the bad line item, never the document — each
  posting becomes its own self-balancing draft. Why: `docs/src/extraction.md`.
- **`verify` uses the `EmailBody` hint on IMAP**, deliberately not penalising a missing one.
  **`EXAMINE`, never `SELECT`.** **`.edu` PARKED** (needs XOAUTH2).
- **Box is adb-only**, credentials do not move, ⛔ `setup.sh` deliberately never written.
  **Prebuilt `dx` is right there** (24.04/glibc 2.39) — the source build in `app-release.yml` is a
  22.04-*runner* constraint. ⚠️ Node from nvm; a bare `npm` there is the **Windows** one.
- 📊 **Measured, do not re-measure:** `gmail_personal` ~**5 msg/day** (12 in 60h) — a real rate,
  not a ceiling. Cap 200/tick, label plain `INBOX`.

## ⚠️ Open
- ⛔ **`UIDVALIDITY` is handled nowhere.** A renumbered mailbox stalls, cursor stuck. Real fix
  carries `uid_validity` in the cursor — trait + schema, own session.
- **The archive purge path** gates enrichment going back on. Enrichment stays OFF on dev.

## ⛔ Inherit
- 🔴 **Locally `fmt`, `clippy`, `check` ONLY — never the full suite.** CI runs it.
- 🔴 **NOTHING since the stamped release (`sha-9a0b1dd`) has run on live.** ⛔ Never deploy there.
- ⚠️ **`omni-me-private` is NOT committed by the session hooks** — current repo only, by hand.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
