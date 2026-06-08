# Build Pipeline

How the Rust/Dioxus frontend becomes the running app — desktop dev and the Android APK.
Read this before changing `package.json` scripts or `src-tauri/tauri.conf.json`'s `build`
block. It exists because the dev-vs-ship mechanism is non-obvious and was misdiagnosed
repeatedly — the answer is **binary embedding, not caching**.

## The frontend is web files

The screens are Rust (Dioxus), compiled to `.wasm` plus a JS loader and an `index.html`.
The editor is hand-written JS (`assets/js/editor.js`) bundled by esbuild into
`assets/js/editor.bundle.js` and loaded at runtime by `components/editor.rs`. The app is a
WebView showing those files.

## Two build tiers, two folders

`dx build` writes to a tier-specific folder and never deletes old output:

| Tier | Folder | wasm | Notes |
|------|--------|------|-------|
| debug | `frontend/target/dx/frontend/debug/web/public` | ~69 MB, unoptimized, `wasm/` layout, `dx-toast` dev tooling | for dev |
| release | `frontend/target/dx/frontend/release/web/public` | ~2.7 MB, optimized, hash-named `assets/` layout | for shipping |

`npm run clean:release` wipes the release `public/` before each release build so only one
hashed wasm exists (else they accumulate and would all get embedded).

## How the Android APK actually serves the frontend  ← the thing that bit us

`tauri-build` **embeds the directory named by `frontendDist` INTO the native binary**
(`libomni_me_app.so`) at compile time. At runtime the `http://tauri.localhost` asset protocol
serves the app **from those embedded bytes**. Two consequences:

1. **`frontendDist` decides what the APK serves** — not the loose `assets/` files. The hashed
   `assets/` that `copy:android:release` copies into the APK are **never served** (dead weight).
2. **There is no cache.** Each build re-embeds. So `clearBrowserCache`, `ignoreCache`,
   reinstall, uninstall, and `?v=` query strings all do *nothing* — the served bytes are whatever
   got compiled into the `.so`.

### The trap

`tauri.conf.json` sets `frontendDist` = the **debug** dir (load-bearing for `cargo tauri dev`).
But a release APK's `beforeBuildCommand` (`npm run build`) builds the **release** dir and
**never touches the debug dir**. So a plain `cargo tauri android build` embeds a **frozen, stale
debug frontend**. Symptom: frontend/JS changes never reach the device no matter how many times
you rebuild + reinstall — only native (Kotlin) changes take effect, because they compile into the
`.so` directly.

### The fix — `scripts/android-build.sh [debug|release]`

Build via the script (default `release`). It overrides `frontendDist`→release **for the build
only**, via `cargo tauri … --config '{"build":{"frontendDist":"…/release/web/public"}}'`, and
pins the memory-safe Cargo profile. The release dir is already built fresh by
`beforeBuildCommand`, so the `.so` embeds the current optimized release frontend. `cargo tauri
dev` still reads `frontendDist`=debug from the file, so the dev flow is untouched.

Verify a build embedded the right tier **before** installing:
`strings libomni_me_app.so | grep -E 'frontend-dxh|wasm/frontend_bg'` → release hash paths
present, debug `/wasm/frontend_bg.wasm` absent. (The `.so` size is a tell too: ~40 MB with the
optimized release wasm vs ~51 MB with the 69 MB debug wasm.)

## Desktop / full-app dev — `cargo tauri dev`

`beforeDevCommand` = `npm run dev` (builds **debug**); no `devUrl`, so `cargo tauri dev` serves
`frontendDist` — which is why it points at debug. UI-only dev uses
`dx serve --platform web --features mock` standalone and ignores all of this (see
`UI_WORKFLOW.md`).

## Dev vs release at a glance

| | Command | builds | serves |
|---|---|---|---|
| UI-only dev | `dx serve --platform web --features mock --port 8080` | debug (dx server) | dx dev server (browser) |
| Full-app dev | `cargo tauri dev` | debug (`npm run dev`) | `frontendDist` = debug |
| Release APK | `scripts/android-build.sh release` | release (`npm run build`) | `frontendDist` override = release, embedded in `.so` |

## npm scripts (`package.json`)

- `build:editor` — esbuild `editor.js` → `editor.bundle.js`.
- `clean:release` — delete the release `public/` (sweep stale hashed output).
- `build:frontend` / `:dev` — `dx build` release / debug.
- `copy:editor:release|dev` — copy the editor bundle into the tier folder.
- `copy:android:release|dev` — copy a tier folder into `gen/android/.../assets` (currently
  redundant for serving — `.so` embedding is authoritative; left in place pending cleanup).
- `dev` = editor + debug frontend + copies. `build` = editor + **release** frontend + copies.

## Deferred cleanup (post open-core split)

- Add a real `devUrl` so `frontendDist` can be release everywhere and the override script
  becomes unnecessary.
- Remove `copy:android:release`/`:dev` if confirmed dead (the `.so` embedding serves).
- Stop committing the generated `editor.bundle.js`.
