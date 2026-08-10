/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{rs,html,js}", "./index.html"],
  theme: {
    extend: {
      colors: {
        // "Blue Topaz" palette, now CSS-var-backed (channels live in
        // input.css :root) so a future light theme is a drop-in swap and every
        // `/NN` opacity tint follows the variable. Values are unchanged from the
        // old hardcoded hex — this refactor is visually byte-for-byte identical.
        'obsidian-bg': 'rgb(var(--color-bg) / <alpha-value>)',
        'obsidian-sidebar': 'rgb(var(--color-sidebar) / <alpha-value>)',
        // Elevated card/panel surface on the main content area (slightly lighter
        // than -bg). New — the Card primitive's canonical surface.
        'obsidian-surface': 'rgb(var(--color-surface) / <alpha-value>)',
        'obsidian-accent': 'rgb(var(--color-accent) / <alpha-value>)',
        'obsidian-text': 'rgb(var(--color-text) / <alpha-value>)',
        'obsidian-text-muted': 'rgb(var(--color-text-muted) / <alpha-value>)',
        // Hairline border token — `border-obsidian-border/10` == the old
        // `border-white/10` idiom, but flips to dark lines in a light theme.
        'obsidian-border': 'rgb(var(--color-border) / <alpha-value>)',
        // Semantic status colors. Additive: existing sites keep their raw
        // green-/amber-/red- utilities; new primitives (Banner, StatTile deltas)
        // consume these, and Stage D migrates the rest onto them.
        'success': 'rgb(var(--color-success) / <alpha-value>)',
        'warn': 'rgb(var(--color-warn) / <alpha-value>)',
        'error': 'rgb(var(--color-error) / <alpha-value>)',
      },
      borderRadius: {
        // One canonical card radius — kills the lg-vs-xl drift across panels.
        'card': '0.75rem',
      },
      boxShadow: {
        'card': '0 1px 2px 0 rgb(0 0 0 / 0.30)',
        'card-hover': '0 6px 16px -4px rgb(0 0 0 / 0.45)',
        // Floating layers: dropdowns, slide-overs, popovers.
        'pop': '0 12px 32px -6px rgb(0 0 0 / 0.55)',
      },
      fontSize: {
        // Body prose scale anchored to the CodeMirror editor (16px / 1.65) so
        // long-form text across the app matches the note/journal reading rhythm.
        'prose': ['1rem', { lineHeight: '1.65' }],
      },
    },
  },
  plugins: [],
}