/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{rs,html,js}", "./index.html"],
  theme: {
    extend: {
      colors: {
        // Initial "Blue Topaz" inspired palette
        'obsidian-bg': '#1e1e1e',
        'obsidian-sidebar': '#161616',
        'obsidian-accent': '#448aff',
        'obsidian-text': '#dcddde',
        'obsidian-text-muted': '#a3a3a3',
      },
    },
  },
  plugins: [],
}