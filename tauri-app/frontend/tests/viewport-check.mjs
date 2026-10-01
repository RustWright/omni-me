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
 * Elements sticking out past the right edge, and containers that scroll sideways.
 *
 * A container meant to scroll sideways (a CSV table, raw text) carries
 * `data-scroll-x` and excuses what is inside it. Nothing else does.
 */
async function overflowReport(page) {
  return page.evaluate(() => {
    const limit = window.innerWidth + 1; // 1px for sub-pixel rounding
    // Excused only by a container that clips, or one marked as meant to scroll
    // sideways. Any `overflow-y-auto` pane computes overflow-x to `auto`, so
    // excusing every scroller excused the whole page: the sweep was blind to the
    // content pane panning, which is the failure users see. Found 2026-10-01.
    const excused = (el) => {
      for (let p = el.parentElement; p; p = p.parentElement) {
        const o = getComputedStyle(p).overflowX;
        if (o === "hidden" || o === "clip") return true;
        if (p.hasAttribute("data-scroll-x")) return true;
      }
      return false;
    };
    // A container that actually scrolls sideways without being marked for it.
    const panners = [];
    for (const el of document.querySelectorAll("body *")) {
      const o = getComputedStyle(el).overflowX;
      if (o !== "auto" && o !== "scroll") continue;
      if (el.scrollWidth <= el.clientWidth + 1) continue;
      if (el.closest("[data-scroll-x]")) continue;
      panners.push({ path: window.__omniPath(el), by: el.scrollWidth - el.clientWidth });
    }
    const insideScroller = excused;
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
      panners: panners.slice(0, 6),
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
// ⛔ What was actually measured, printed on success. A pass that does not say
// what it looked at is indistinguishable from a pass that looked at nothing —
// and the deep archive check, which covers the one screen that has really
// overflowed, is skipped silently when its fixture is missing.
const measured = [];

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
      measured.push(label);
      if (rep.pans || rep.offenders.length || rep.panners.length) {
        failures.push({ where: label, ...rep });
      }
      for (const t of await tapTargetReport(page, MIN_TAP_PX)) {
        notes.push({ where: label, ...t });
      }

      // One level deeper on Archive: the list is well-behaved and the detail
      // view is what actually broke. `doc-wide-statement` is the mock fixture
      // that exists to make this measurable.
      if (tab === "archive") {
        const doc = page.locator("text=chequing-2026-02-full-export_AllAccounts__20260228_000123456789.csv").first();
        if (await doc.count()) {
          await doc.click();
          await page.waitForTimeout(600);
          const deep = await overflowReport(page);
          measured.push(`${label} › detail (wide CSV)`);
          if (deep.pans || deep.offenders.length || deep.panners.length) {
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
    for (const p of f.panners ?? []) {
      console.error(`      scrolls sideways by ${p.by}px — ${p.path}`);
    }
  }
  console.error(`\n${failures.length} screen(s) overflow. Nothing should pan sideways.`);
  process.exit(1);
}

const deepRan = measured.filter((m) => m.includes("detail")).length;
console.log(`\nMeasured ${measured.length} screens:`);
for (const m of measured) console.log(`  ${m}`);
if (deepRan === 0) {
  // ⛔ Fatal, not a note. The archive detail view is the one screen that has
  // actually overflowed; a run that never reached it has checked everything
  // except the thing this was written for, and saying "no overflow" would be a
  // green built on an untested screen.
  console.error("\nThe archive detail view was never reached — nothing verified the case this exists for.");
  process.exit(1);
}
console.log("\nNo horizontal overflow at any checked width.");
