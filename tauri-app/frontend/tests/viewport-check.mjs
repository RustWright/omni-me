// Phone-width layout sweep: drive every tab at a phone viewport and fail if the
// page can be scrolled sideways.
//
// Why this exists: the archive detail view shipped overflowing the screen and
// nothing caught it. The mechanism was a CSS grid item at its default
// `min-width: auto`, which refuses to shrink below its content — so the inner
// viewer's `overflow-auto` never engaged and the whole page panned instead.
// That class of bug is invisible in review and obvious in one measurement.
//
// Run against a served `--features mock` web build:
//   node tests/viewport-check.mjs http://127.0.0.1:8080
//
// Exit code 1 means something overflowed. Tap-target findings are reported but
// never fail the run: shrinking a control is a bug, enlarging one is a design
// decision, and only the first belongs in a gate.

import { chromium } from "playwright";

const BASE = process.argv[2] ?? "http://127.0.0.1:8080";

// Two real Android widths. 360 is the narrowest in common use and is where a
// layout that merely fits at 390 gives out.
const VIEWPORTS = [
  { name: "360x740", width: 360, height: 740 },
  { name: "390x844", width: 390, height: 844 },
];

// `as_key()` in main.rs; the drawer renders these as `data-tab`.
const TABS = [
  "journal",
  "notes",
  "assistant",
  "routines",
  "finances",
  "archive",
  "settings",
];

// Android's minimum touch target is 48dp and iOS's is 44pt. 44 is the lower of
// the two and the one worth reporting against — below it, misfires are the
// control's fault rather than the finger's.
const MIN_TAP_PX = 44;

/** A short, human-readable path to an element, for a report someone has to act on. */
function cssPathSource() {
  return `
    window.__omniPath = (el) => {
      const parts = [];
      while (el && el.nodeType === 1 && parts.length < 4) {
        let part = el.tagName.toLowerCase();
        if (el.id) { part += '#' + el.id; }
        else {
          const cls = (el.getAttribute('class') || '')
            .split(/\\s+/).filter(Boolean).slice(0, 3).join('.');
          if (cls) part += '.' + cls;
        }
        parts.unshift(part);
        el = el.parentElement;
      }
      return parts.join(' > ');
    };
  `;
}

/**
 * Elements sticking out past the right edge.
 *
 * Anything inside a scroll container is skipped: a wide CSV table inside a box
 * that scrolls is working as designed, and reporting it would bury the real
 * finding under every legitimately-scrollable child.
 */
async function overflowReport(page) {
  return page.evaluate(() => {
    const limit = window.innerWidth + 1; // 1px for sub-pixel rounding
    const scrolls = (el) => {
      const o = getComputedStyle(el).overflowX;
      return o === "auto" || o === "scroll" || o === "hidden" || o === "clip";
    };
    const insideScroller = (el) => {
      for (let p = el.parentElement; p; p = p.parentElement) {
        if (scrolls(p)) return true;
      }
      return false;
    };
    const offenders = [];
    for (const el of document.querySelectorAll("body *")) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      if (r.right <= limit) continue;
      if (insideScroller(el)) continue;
      offenders.push({ path: window.__omniPath(el), right: Math.round(r.right) });
    }
    // The outermost offender explains the inner ones; keep a few, not hundreds.
    const seen = new Set();
    const unique = offenders.filter((o) => {
      if (seen.has(o.path)) return false;
      seen.add(o.path);
      return true;
    });
    const doc = document.scrollingElement;
    return {
      pans: doc.scrollWidth > doc.clientWidth + 1,
      scrollWidth: doc.scrollWidth,
      clientWidth: doc.clientWidth,
      offenders: unique.slice(0, 6),
    };
  });
}

/** Interactive controls smaller than a fingertip. Reported, never fatal. */
async function tapTargetReport(page, minPx) {
  return page.evaluate((min) => {
    const out = [];
    for (const el of document.querySelectorAll("button, a[href], [role=button]")) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      if (r.height >= min && r.width >= min) continue;
      out.push({
        path: window.__omniPath(el),
        label: (el.textContent || "").trim().slice(0, 32),
        w: Math.round(r.width),
        h: Math.round(r.height),
      });
    }
    return out.slice(0, 12);
  }, minPx);
}

/**
 * The drawer is always in the DOM — its visibility is class-toggled so the slide
 * can animate — so "is it open" is a class check, not a presence check.
 */
async function drawerOpen(page) {
  return page
    .locator("aside.fixed")
    .first()
    .evaluate((el) => el.className.includes("translate-x-0"))
    .catch(() => false);
}

async function openTab(page, tab) {
  // The SideNav is off-viewport at phone widths (`md:hidden` swap), so the drawer
  // is the only way in. ⛔ Clicking the hamburger while the drawer is already open
  // targets a button underneath it and times out against the panel intercepting
  // pointer events — which reads as "the app is broken" rather than "the script
  // is confused".
  if (!(await drawerOpen(page))) {
    await page.click('[aria-label="Open navigation"]');
  }
  await page.click(`aside [data-tab="${tab}"]`);
  // The drawer animates out over 200ms; measuring through it reports the
  // translating panel rather than the page.
  await page.waitForTimeout(400);
}

const failures = [];
const notes = [];

const browser = await chromium.launch();
try {
  for (const vp of VIEWPORTS) {
    const page = await browser.newPage({
      viewport: { width: vp.width, height: vp.height },
    });

    // ⛔ Collected so a dead app names itself. When the wasm aborts — which it
    // does on the first tab switch if `editor.bundle.js` is missing — every
    // subsequent click times out against a page that has stopped responding, and
    // the bare Playwright error blames the selector. These lines carry the cause.
    // Two dx artifacts are filtered: the flapping hot-reload socket and its toast
    // helper, neither of which is an app fault.
    const pageErrors = [];
    const isDxNoise = (t) =>
      t.includes("_dioxus?build_id") || t.includes("showDXToast");
    page.on("pageerror", (e) => {
      if (!isDxNoise(String(e))) pageErrors.push(String(e));
    });
    page.on("console", (m) => {
      if (m.type() === "error" && !isDxNoise(m.text())) pageErrors.push(m.text());
    });

    await page.addInitScript(cssPathSource());
    await page.goto(BASE, { waitUntil: "networkidle" });
    // The wasm bundle boots after load; without this the first measurement is
    // of an empty shell and passes for the wrong reason.
    await page.waitForSelector('[aria-label="Open navigation"]', { timeout: 30_000 });

    for (const tab of TABS) {
      const label = `${vp.name} ${tab}`;
      try {
        await openTab(page, tab);
      } catch (e) {
        // ⛔ Never swallowed into a pass. A tab the sweep could not open is a
        // screen it did not measure, and reporting "no overflow" for it would be
        // the false green this whole check exists to prevent.
        console.error(`\nCould not open ${label}: ${e.message.split("\n")[0]}`);
        if (pageErrors.length) {
          console.error("  the page had already failed:");
          for (const p of [...new Set(pageErrors)].slice(0, 5)) {
            console.error(`    ${p}`);
          }
        }
        failures.push({
          where: label,
          scrollWidth: 0,
          clientWidth: 0,
          offenders: [{ path: "(tab unreachable — not measured)", right: 0 }],
        });
        break;
      }
      const rep = await overflowReport(page);
      if (rep.pans || rep.offenders.length) {
        failures.push({ where: label, ...rep });
      }
      for (const t of await tapTargetReport(page, MIN_TAP_PX)) {
        notes.push({ where: label, ...t });
      }

      // One level deeper on Archive: the list is well-behaved and the detail
      // view is what actually broke. `doc-wide-statement` is the mock fixture
      // that exists to make this measurable.
      if (tab === "archive") {
        const doc = page.locator("text=chequing-2026-02-full-export.csv").first();
        if (await doc.count()) {
          await doc.click();
          await page.waitForTimeout(600);
          const deep = await overflowReport(page);
          if (deep.pans || deep.offenders.length) {
            failures.push({ where: `${label} › detail`, ...deep });
          }
        } else {
          notes.push({
            where: label,
            label: "mock fixture doc-wide-statement not listed",
            w: 0,
            h: 0,
            path: "(the deep check did not run)",
          });
        }
      }
    }
    await page.close();
  }
} finally {
  await browser.close();
}

if (notes.length) {
  console.log(`\nTap targets under ${MIN_TAP_PX}px (reported, not fatal):`);
  for (const n of notes) {
    console.log(`  ${n.where}: ${n.w}x${n.h} "${n.label}" — ${n.path}`);
  }
}

if (failures.length) {
  console.error("\nHorizontal overflow:");
  for (const f of failures) {
    console.error(`  ${f.where}: page ${f.scrollWidth}px wide in ${f.clientWidth}px`);
    for (const o of f.offenders) {
      console.error(`      reaches ${o.right}px — ${o.path}`);
    }
  }
  console.error(`\n${failures.length} screen(s) overflow. Nothing should pan sideways.`);
  process.exit(1);
}

console.log("\nNo horizontal overflow at any checked width.");
