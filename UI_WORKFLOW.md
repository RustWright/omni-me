# UI Development Workflow

How to develop and iterate on the omni-me UI with LLM assistance.

## Overview

The UI can be developed in two modes:

| Mode | Command | Use Case |
|------|---------|----------|
| **UI-only** | `dx serve --platform web --features mock --open false --port 8080` | Styling, layout, component design. No backend needed. |
| **Full app** | `cargo tauri dev` (from `tauri-app/src-tauri/`) | Integration testing, real data, IPC verification. |

UI-only mode serves the Dioxus frontend to a browser on `localhost:8080` with mock data. The LLM uses Playwright MCP tools to navigate, click, take screenshots, and read accessibility trees — enabling autonomous visual iteration without manual screenshots.

## Quick Start (UI-only mode)

```bash
# Terminal 1: Start the UI dev server
cd tauri-app/frontend
dx serve --platform web --features mock --open false --port 8080
```

The LLM then:
1. Navigates Playwright to `http://localhost:8080`
2. Takes screenshots and reads accessibility snapshots
3. Edits `.rs` files with Tailwind classes
4. Dioxus hot-reloads the change
5. Screenshots again to verify
6. Repeats until the UI matches the target

## Architecture

```
tauri-app/
  frontend/
    src/
      main.rs           ← App shell, tab routing, Tailwind link
      components/
        nav.rs           ← Bottom navigation bar
        editor.rs        ← CodeMirror wrapper (works in browser + Tauri)
      pages/
        journal.rs       ← Journal list, search, editor views
        routines.rs      ← Daily checklist, group management
        settings.rs      ← Sync config, timezone
      bridge.rs          ← Tauri IPC functions + #[cfg(feature = "mock")] blocks
      types.rs           ← Shared data types (NoteListItem, RoutineGroup, etc.)
      user_date.rs       ← Timezone-aware date utilities
    Dioxus.toml          ← Tailwind config (input/output paths)
    input.css            ← Tailwind directives + base body styles
    tailwind.config.js   ← Color palette (Obsidian Blue Topaz theme)
    assets/
      tailwind.css       ← Generated (gitignored)
```

## Styling

- **Engine:** TailwindCSS via Dioxus integration
- **Theme:** Obsidian Blue Topaz dark theme
- **Colors:** Defined in `tailwind.config.js`:
  - `obsidian-bg: #1e1e1e` (main background)
  - `obsidian-sidebar: #161616` (cards, panels)
  - `obsidian-accent: #448aff` (active elements, links)
  - `obsidian-text: #dcddde` (primary text)
  - `obsidian-text-muted: #a3a3a3` (secondary text)
- **Pattern:** Use Tailwind utility classes directly in `rsx!` macros. No CSS modules.
- **Responsive:** Use `md:`, `lg:` breakpoints. Mobile-first.

## Mock System

The `mock` feature flag in `tauri-app/frontend/Cargo.toml` enables browser-only development:

- `#[cfg(feature = "mock")]` blocks in `bridge.rs` return static data for every IPC call
- `#[cfg(not(feature = "mock"))]` blocks call real Tauri `invoke()` commands
- Mock data includes realistic examples (journal entries, routine groups, completions)

**Limitations:** Mock data is static. State changes (saving, completing items) return success but don't update the displayed data. This is acceptable for visual development — use `cargo tauri dev` for interaction testing.

## LLM Instructions

When working on the UI:

1. **Read this file** to understand the workflow and architecture
2. **Start `dx serve`** with mock features if not already running
3. **Use Playwright MCP tools** to see the current state:
   - `browser_navigate` to `http://localhost:8080`
   - `browser_take_screenshot` for visual review
   - `browser_snapshot` for accessibility tree (element refs for clicking)
   - `browser_click` to navigate between views
4. **Edit the relevant `.rs` file** in `frontend/src/pages/` or `frontend/src/components/`
5. **Wait for hot-reload** (Dioxus rebuilds automatically)
6. **Screenshot again** to verify the change
7. **Iterate** until the UI matches the target aesthetic and layout

### Component Design Pattern

When building new UI components, use HTML from component libraries (Flowbite, Preline, DaisyUI) as reference and translate to `rsx!` macros with Tailwind classes:

```rust
rsx! {
    div { class: "bg-obsidian-sidebar rounded-xl p-4 border border-white/10",
        h3 { class: "text-obsidian-text font-semibold mb-2", "Title" }
        p { class: "text-obsidian-text-muted text-sm", "Description" }
    }
}
```

## Verification Checklist

See `ui-checklist.md` for the full interaction checklist with current test results.

## Build Pipeline

| Script | Purpose |
|--------|---------|
| `npm run dev` | Debug build: editor bundle + WASM + copy assets |
| `npm run build` | Release build (requires wasm-opt) |
| `npm run build:editor` | Bundle CodeMirror JS only |

The `beforeDevCommand` in `tauri.conf.json` runs `npm run dev` automatically when `cargo tauri dev` is invoked.
