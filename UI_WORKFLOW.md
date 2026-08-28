# UI Development Workflow

How to develop and iterate on the omni-me UI. Read this before any UI work.

**Last verified against the code:** 2026-08-28.

## Two modes

| Mode | Command | Use for |
|------|---------|---------|
| **UI-only (browser)** | `dx serve --platform web --features mock --port 8080` from `tauri-app/frontend/` | Styling, layout, component design. No backend. |
| **Full app** | `cargo tauri dev` from `tauri-app/src-tauri/` | Integration, real data, IPC, anything the mock can't reach. |

Browser mode serves the Dioxus frontend on `localhost:8080` with a mocked IPC bridge, and
Playwright MCP drives it — navigate, click, screenshot, read the accessibility tree — so
visual iteration needs no manual screenshotting.

For a real bundle use the committed scripts rather than `cargo tauri build` directly:
`tauri-app/scripts/desktop-build.sh` (release; extra args are forwarded to
`cargo tauri build`) and `tauri-app/scripts/android-build.sh [debug|release]`, default
release. They exist because `tauri.conf.json` pins `frontendDist` to the **debug**
directory for `cargo tauri dev`, while `beforeBuildCommand` populates **release** — so a
plain `cargo tauri build` embeds whatever a UI session last left in debug, which is a
`--features mock` build. That is exactly how an APK full of mock data once shipped. Both
scripts override `frontendDist` and then run the mock-sentinel check on the result.

---

## Layout

```
tauri-app/
  assets/js/editor.js        ← CodeMirror 6 editor (the only non-Rust UI source)
  frontend/
    input.css                ← CSS custom-property token layer + base styles
    tailwind.config.js       ← maps Tailwind names onto those tokens
    Dioxus.toml
    src/
      main.rs                ← app shell, persistent sub-nav, auto-hide-on-scroll header
      bridge.rs              ← every invoke_* wrapper + its #[cfg(feature = "mock")] twin
      types.rs               ← wire structs mirroring the backend
      autosave.rs            ← SaveState + retry/backoff shared by both editors
      continuity.rs          ← draft/list-state persistence across navigation
      user_date.rs           ← timezone-aware date helpers
      components/
        primitives.rs        ← design system: Button, Card, Banner, PageHeader,
                               SegmentedNav, TextInput/INPUT_CLASS, IconButton
        icon.rs              ← icon set
        nav.rs               ← SideNav (desktop) + drawer (mobile)
        editor.rs            ← CodeMirror wrapper (browser + Tauri)
        month_grid.rs        ← shared month cells + prev_month/next_month
        date_field.rs        ← the app-wide date input (never native <input type=date>)
        account_input.rs, tag_editor.rs, sync_status.rs
      pages/
        journal.rs, notes.rs, routines.rs, finances.rs, settings.rs, import_export.rs
```

`finances.rs` is one ~7,000-line file covering three surfaces (Overview / Ledger /
Analyze) behind `surface_of()`. Splitting it is deliberately deferred — see the trip-wire
in its module header.

---

## Styling

- **Tailwind**, mobile-first, utility classes directly in `rsx!`. No CSS modules.
- **Tokens live in `input.css`** as CSS custom properties (`--color-bg`,
  `--color-accent`, …) in space-separated RGB form, so Tailwind can apply opacity
  modifiers: `rgb(var(--color-accent) / 0.1)`. `tailwind.config.js` maps the friendly
  names onto them. A light theme would be one extra `:root[data-theme="light"]` block and
  nothing else.
- Prefer a primitive from `components/primitives.rs` over hand-rolled classes. If you
  find yourself writing a button's class string, use `Button`.

---

## Gotchas that have each cost real debugging time

**`dx serve` hot-reload is partial.** It applies dynamic and attribute changes, but
silently keeps **stale static class literals and structural changes** on already-rendered
components — reloading the page still shows the old classes. If a class or layout edit
"doesn't take", restart `dx serve`.

**Editing `editor.js` does nothing on its own.** `dx serve` serves a *copied* bundle.
After editing it:

```bash
cd tauri-app && npm run build:editor && npm run copy:editor:dev
```

Only `assets/js/editor.bundle.js` is tracked; the desktop/Android copies are regenerated,
not committed.

**Native form controls are NOT Playwright-verifiable.** Playwright drives Chromium; the
desktop app runs **webkit2gtk**. They disagree on `<select>`, scrollbars and date pickers.
A `color-scheme: dark` fix was signed off from a Chromium screenshot and shipped broken —
webkit2gtk kept painting closed `<select>` boxes with the light GTK theme. The working
approach is `appearance: none` plus explicit background/color (see the global `select`
rule in `input.css`). **Never close a native-control fix off a screenshot**; it rides the
on-device pass.

**At 390px the desktop SideNav is off-viewport** (`md:hidden` swap), so a `browser_click`
on a nav item fails with "element is outside of the viewport". Open the hamburger drawer
first, or verify navigation at 1280px.

**`browser_type` with a `target` uses `.fill()` semantics** — it *replaces*, not appends.
Into CodeMirror that wipes and sets the whole body.

**Testing scroll behaviour needs a real scroll range.** `onscroll` works in dioxus-web
0.7, but a page that barely scrolls plus one `scrollTop = 999` jump does not drive
direction logic and reads as broken. Use a short viewport (e.g. 900×300) and incremental
`scrollTop` steps across animation frames.

**Two console messages are dx artifacts, not app bugs:** a flapping
`ws://…/_dioxus?build_id=0` connection failure, and a `TypeError` in `showDXToast`.
Filter those before declaring the console clean — real errors name your own modules.

**Screenshots**: `browser_take_screenshot`'s `filename` is relative to the repo cwd, so
PNGs land at the repo root. They're gitignored (`/*.png`), but delete them after
verifying.

**Mock limits.** The mock bridge only returns *today's* journal entry (any other date
gives a template) and never `closed`/`complete`, so day-completion and closed-day
read-only behaviour **cannot** be verified in the browser. Cover that logic with unit
tests and confirm on-device.

---

## Dioxus 0.7 specifics

- A component parameter literally named `props` breaks `#[component]` — the macro
  generates its own `props` binding. Rename it.
- A bare `key:` on a directly-rendered component is a **no-op**. Dioxus honours `key` only
  inside a list. To force a remount, render through a one-element list:
  `for v in std::iter::once(x) { Comp { key: "{v}", .. } }`.
- `use_effect` re-runs on subscribed *signal* reads, not on prop changes. `Editor` relies
  on this: it seeds CodeMirror once from `initial_content` and ignores later updates.

---

## Verification

Run all of these before calling UI work done. Note the frontend is a **separate
workspace** — no root-level `-p` flag reaches it, so these run from its own directory.

```bash
cd tauri-app/frontend
cargo test                                                              # host-native
cargo clippy --target wasm32-unknown-unknown --features mock -- -D warnings
cargo clippy --target wasm32-unknown-unknown            -- -D warnings
```

Both clippy configs matter: `mock` compiles a different half of `bridge.rs`, so a clean
lint in one says nothing about the other. CI now runs all three.

Then Playwright at **390px and 1280px**, with 0 app console errors (after filtering the
two dx artifacts above). Backend-dependent behaviour stays "web-verified, on-device
pending" until it runs against a real backend.

`ui-checklist.md` holds the interaction checklist.

---

## Build pipeline

| Script | Does |
|--------|------|
| `npm run dev` | editor bundle → debug WASM → copy editor + Android assets |
| `npm run build` | release WASM → **`assert:no-mock`** → copy editor + Android assets |
| `npm run build:editor` | bundle CodeMirror only (esbuild) |

`assert-no-mock.sh` greps the built artifact for a sentinel compiled in whenever the
`mock` feature is on. It exists because a mock build once shipped inside an APK — and
note *why* a compile-time guard wouldn't have caught it: `dx serve --features mock`
builds in debug, so `debug_assertions` is true and any `cfg`-based guard stays silent;
the failure was in which directory got copied, not in how the code was compiled.
