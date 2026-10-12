// Every Tailwind class written in the source must exist in the built stylesheet.
//
// Tailwind ignores a class it does not know, silently. A bulk edit once glued
// `z-30flex` and `bg-obsidian-bgflex` together on five lines, and the bars lost
// their layout with nothing failing. Run after `dx build`, which writes the CSS.
//
// A string literal counts as a class list when most of its words are known
// classes; its other words are then reported. Words with `{` are interpolated
// and skipped.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const css = readFileSync(join(root, "assets/tailwind.css"), "utf8");

const known = new Set();
// Escapes are a backslash and one character, or a hex code point and a space:
// `grid-cols-[repeat(7,1fr)]` is written `grid-cols-\[repeat\(7\2c 1fr\)\]`.
for (const m of css.matchAll(/\.((?:\\[0-9a-fA-F]{1,6} ?|\\.|[A-Za-z0-9_-])+)/g)) {
  known.add(
    m[1]
      .replace(/\\([0-9a-fA-F]{1,6}) ?/g, (_, h) => String.fromCodePoint(parseInt(h, 16)))
      .replace(/\\(.)/g, "$1"),
  );
}
// Classes styled outside Tailwind's generated selectors, or used only as hooks.
for (const extra of ["group", "peer", "dark", "sticky-composer", "thinking-dot"]) {
  known.add(extra);
}

function files(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : p.endsWith(".rs") ? [p] : [];
  });
}

const problems = [];
for (const file of files(join(root, "src"))) {
  const lines = readFileSync(file, "utf8").split("\n");
  lines.forEach((line, i) => {
    for (const m of line.matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
      const words = m[1].split(/\s+/).filter((w) => w && !w.includes("{"));
      if (words.length < 3) continue;
      const hits = words.filter((w) => known.has(w)).length;
      if (hits / words.length < 0.6) continue;
      for (const w of words) {
        if (!known.has(w)) problems.push(`${file.slice(root.length)}:${i + 1}  ${w}`);
      }
    }
  });
}

if (problems.length) {
  console.error("Classes not in the built stylesheet (a typo, or a glued pair):");
  for (const p of problems) console.error(`  ${p}`);
  process.exit(1);
}
console.log(`class check: every class in a class list is in the stylesheet (${known.size} known)`);
