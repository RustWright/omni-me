# UI/UX Design Strategy & Autonomous Workflow

**Status:** Initializing (Cycle 2)
**Target Aesthetic:** Obsidian (Blue Topaz theme) - clean, highly functional, dense but readable.
**Target Platforms:** Mobile-first, but robust and responsive for laptop/desktop use.

## Core Architecture

1.  **Dioxus handles 100% of the UI:** Tauri is strictly for native OS integrations (window management, system tray, etc.). All layouts, buttons, views, and state are centralized within Dioxus in the `frontend` crate.
2.  **Styling Engine:** TailwindCSS.
    *   No complex CSS modules unless absolutely necessary.
    *   Raw HTML UI components (like from Preline, Flowbite, or DaisyUI) can be copied and directly translated to `rsx!` macros.
3.  **Responsive Design:** Use Tailwind's `md:`, `lg:` breakpoints to ensure the UI scales elegantly from a mobile phone to a full desktop monitor.

## The Autonomous AI Loop

To minimize manual user involvement in pixel-pushing, we employ an autonomous feedback loop using the AI's native vision capabilities.

1.  **Directives:** The user provides a high-level UI goal (e.g., "Build the Routine Manager list view. Make it look like an Obsidian dataview table.").
2.  **Implementation:** AI edits the Dioxus `.rs` files using Tailwind utility classes.
3.  **Tauri Hot Reload:** `cargo tauri dev` automatically rebuilds and refreshes the WebView.
4.  **Vision Capture:** An automated script in the `ui_dev/` directory (using Puppeteer) takes a headless screenshot of the rendered UI and saves it as `ui_state.png`.
5.  **AI Vision Analysis:** The AI ingests `ui_state.png`, compares the visual output to the user's prompt and the target aesthetic (Obsidian Blue Topaz), and self-critiques.
6.  **Iteration:** If alignment, padding, or colors are off, the AI autonomously applies fixes and triggers the loop again.
7.  **Delivery:** Once the AI is satisfied the UI meets the criteria, it presents the result to the user for final critique.

## Reference Aesthetic: Obsidian "Blue Topaz"

When designing components, we must aim for parity with the Obsidian experience:
*   **Typography:** highly readable, proper hierarchical scaling.
*   **Layout:** Information-dense but uncluttered.
*   **Colors:** Deep/clean backgrounds with specific accent colors (we will refine the exact hex codes as we build).
*   **Interactivity:** Fast, desktop-class responsiveness even on mobile.

## Development Tools

All scripts, tooling, and temporary state for this UI workflow are housed in the `ui_dev/` directory to keep the main application tree clean.

*   `ui_dev/take_screenshot.js`: Puppeteer script to capture the current state of the UI.
*   `ui_dev/debug_editor.js`: Puppeteer script for debugging editor loading issues.
*   `ui_dev/ui_state.png`: The latest visual representation of the application used by the AI.

**Development Workflow:**
1.  Run `cargo tauri dev --features mock` from the project root.
2.  The application will launch in a Tauri window, serving the Dioxus frontend with mock data.
3.  The Puppeteer scripts (`debug_editor.js`, `take_screenshot.js`) will connect to the Tauri dev server (typically `http://localhost:1420`).