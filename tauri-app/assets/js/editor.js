import { EditorView, minimalSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, RangeSetBuilder, Annotation } from "@codemirror/state";
import { Decoration, ViewPlugin, WidgetType } from "@codemirror/view";

// ---------------------------------------------------------------------------
// Editor typography theme (Phase 5 "editor feel")
// ---------------------------------------------------------------------------
//
// The notes/journal are *prose*, but CodeMirror's default `.cm-content` is
// `monospace` and minimalSetup's default markdown highlight style underlines
// headings/links. That read like code and used space poorly vs Obsidian. This
// theme makes the writing surface proportional, larger, and airy, kills the
// syntax underline, and lets the editor grow to fill its (flex) parent so the
// page — not a fixed 400px island — is the writing area.
//
// The editor deliberately does NOT own its scroll (`.cm-scroller` stays
// overflow-visible): the page content column remains the single scroll parent,
// which is what the keyboard-inset caret logic below (1.10) relies on. `flex`
// on `&` lets a short note stretch to fill while a long note grows past the
// viewport and the page column scrolls — same scroll model as before.
const SANS_STACK =
  'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';

const omniEditorTheme = EditorView.theme({
  "&": {
    flex: "1 0 auto",
    fontSize: "16px",
    color: "#dcddde",
  },
  ".cm-scroller": {
    fontFamily: SANS_STACK,
    lineHeight: "1.65",
  },
  ".cm-content": {
    fontFamily: SANS_STACK,
    // Minimal horizontal inset — the page column supplies the gutter. Some
    // bottom breathing room so the last line isn't glued to the edge.
    padding: "10px 2px 48px",
    caretColor: "#dcddde",
  },
  ".cm-line": {
    padding: "0 4px",
  },
  "&.cm-focused": {
    outline: "none",
  },
  // Kill CM's default markdown heading/link underline (source of the per-line
  // underline artifact). Higher specificity than the generated token class, so
  // no !important needed.
  ".cm-content .cm-line span": {
    textDecoration: "none",
  },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "#448aff",
    borderLeftWidth: "2px",
  },
  // #344 reveal-on-select line completion time: floated to the right edge of the
  // active line, small and muted so it reads as metadata, never selectable.
  ".cm-ts-reveal": {
    float: "right",
    marginLeft: "1.5em",
    color: "#6f747d",
    fontSize: "0.72em",
    lineHeight: "1.65",
    letterSpacing: "0.02em",
    fontVariantNumeric: "tabular-nums",
    userSelect: "none",
    pointerEvents: "none",
    whiteSpace: "nowrap",
  },
});

let editorView = null;
// Set when a journal editor is created (#344): stamps the final in-progress line
// on teardown, before the view is destroyed. null for non-journal editors.
let timestampFlush = null;
let isDirty = false;
// Sticky "the user has edited this editor instance at least once". Unlike
// `isDirty`, this is NOT cleared by markClean()/autosave — only reset when a
// fresh editor is created (createEditor). It exists so live sync-refresh can
// tell "the user has been working in here this session" from "clean right now
// because autosave just ran": the latter would otherwise let an incoming remote
// edit clobber text the user is actively typing between autosaves.
let everDirty = false;
let suppressDirty = false;
const dirtyListeners = [];
const cleanListeners = [];

// ---------------------------------------------------------------------------
// 1.4 - Dirty / Clean signalling
// ---------------------------------------------------------------------------

function emitDirty() {
  // Only ever reached on a genuine USER edit: the update listener gates this
  // behind `!suppressDirty`, and setEditorContent (programmatic writes) sets
  // suppressDirty. So this is the right place to latch the sticky flag.
  everDirty = true;
  if (isDirty) return;
  isDirty = true;
  for (const cb of dirtyListeners) {
    try {
      cb();
    } catch (e) {
      console.error("editorEvents.onDirty listener threw:", e);
    }
  }
}

function emitClean() {
  if (!isDirty) return;
  isDirty = false;
  for (const cb of cleanListeners) {
    try {
      cb();
    } catch (e) {
      console.error("editorEvents.onClean listener threw:", e);
    }
  }
}

window.editorEvents = {
  onDirty(cb) {
    if (typeof cb === "function") dirtyListeners.push(cb);
  },
  onClean(cb) {
    if (typeof cb === "function") cleanListeners.push(cb);
  },
  isDirty() {
    return isDirty;
  },
  // Sticky across autosave; reset only on createEditor. See `everDirty` decl.
  everDirty() {
    return everDirty;
  },
};

// Entry point so the Rust side can flip state back to clean after a save.
window.markClean = function () {
  emitClean();
};

// ---------------------------------------------------------------------------
// 1.1 - Auto-wrap pairs
// ---------------------------------------------------------------------------

// Character pairs we auto-wrap / auto-pair.
const PAIRS = {
  '"': '"',
  "'": "'",
  "(": ")",
  "[": "]",
  "{": "}",
  "*": "*",
  _: "_",
  "`": "`",
};

const WORD_CHAR_RE = /[A-Za-z0-9]/;

/**
 * Single-quote rule:
 *   Do NOT auto-pair `'` when the character immediately before the cursor is a
 *   word character (letter or digit). This covers contractions like "don't",
 *   "it's", "I'm" - where the user is typing a possessive / contraction
 *   apostrophe inside a word, not opening a quotation.
 *   If the user makes a selection and presses `'`, wrap always happens (the
 *   intent is unambiguous).
 *
 *   Symmetric characters `"`, `*`, `_`, `` ` `` do NOT get this treatment.
 */
function shouldSkipSingleQuote(state, from) {
  if (from <= 0) return false;
  const before = state.doc.sliceString(from - 1, from);
  return WORD_CHAR_RE.test(before);
}

const autoWrapFilter = EditorState.transactionFilter.of((tr) => {
  // Ignore anything that isn't a plain user input insertion.
  if (!tr.isUserEvent("input.type") && !tr.isUserEvent("input")) {
    return tr;
  }
  if (!tr.docChanged) return tr;

  // We only care when the user typed exactly one of our trigger characters.
  let inserted = null;
  let insertFrom = null;
  let insertTo = null;
  let multipleChanges = false;

  tr.changes.iterChanges((fromA, toA, _fromB, _toB, insert) => {
    if (multipleChanges) return;
    if (inserted !== null) {
      multipleChanges = true;
      return;
    }
    inserted = insert.toString();
    insertFrom = fromA;
    insertTo = toA;
  });

  if (multipleChanges || inserted === null) return tr;
  if (inserted.length !== 1) return tr;

  const closer = PAIRS[inserted];
  if (closer === undefined) return tr;

  const state = tr.startState;
  const selection = state.selection.main;
  const selectedText = state.sliceDoc(selection.from, selection.to);

  // Case A: selection exists -> wrap it with opener + selected + closer.
  if (selectedText.length > 0) {
    if (insertFrom !== selection.from || insertTo !== selection.to) {
      return tr;
    }
    return [
      {
        changes: {
          from: selection.from,
          to: selection.to,
          insert: inserted + selectedText + closer,
        },
        // Keep the original text selected (between the newly inserted pair).
        selection: {
          anchor: selection.from + 1,
          head: selection.from + 1 + selectedText.length,
        },
      },
    ];
  }

  // Case B: no selection -> insert pair and put cursor between.
  if (inserted === "'" && shouldSkipSingleQuote(state, insertFrom)) {
    return tr;
  }

  if (insertFrom !== insertTo) return tr;
  if (insertFrom !== selection.from) return tr;

  return [
    {
      changes: {
        from: insertFrom,
        to: insertFrom,
        insert: inserted + closer,
      },
      selection: { anchor: insertFrom + 1 },
    },
  ];
});

// ---------------------------------------------------------------------------
// 1.2 - `- [ ] ` checkbox rendering
// ---------------------------------------------------------------------------

// Match a checkbox prefix at the start of a line: `- [ ] ` or `- [x] `.
// Captures the inner mark ([ ] or [x]) so we can toggle it on click.
const CHECKBOX_RE = /^(\s*)-\s\[([ xX])\]\s/;

class CheckboxWidget extends WidgetType {
  constructor(checked, markFrom) {
    super();
    this.checked = checked;
    this.markFrom = markFrom;
  }
  eq(other) {
    return other.checked === this.checked && other.markFrom === this.markFrom;
  }
  toDOM() {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = this.checked;
    input.className = "cm-checkbox-widget";
    input.style.marginRight = "6px";
    input.style.cursor = "pointer";
    input.style.verticalAlign = "middle";
    input.dataset.markFrom = String(this.markFrom);
    input.dataset.checked = this.checked ? "1" : "0";
    return input;
  }
  ignoreEvent() {
    return false;
  }
}

function buildCheckboxDecorations(view) {
  const builder = new RangeSetBuilder();
  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = view.state.doc.lineAt(pos);
      const m = line.text.match(CHECKBOX_RE);
      if (m) {
        const indent = m[1].length;
        const markCharPos = line.from + indent + 3; // position of ' ' or 'x'
        const replaceFrom = line.from + indent; // start of "- ["
        const replaceTo = line.from + indent + 6; // end of "] "
        const checked = m[2] === "x" || m[2] === "X";
        builder.add(
          replaceFrom,
          replaceTo,
          Decoration.replace({
            widget: new CheckboxWidget(checked, markCharPos),
          }),
        );
      }
      if (line.to >= to) break;
      pos = line.to + 1;
    }
  }
  return builder.finish();
}

const checkboxPlugin = ViewPlugin.fromClass(
  class {
    constructor(view) {
      this.decorations = buildCheckboxDecorations(view);
    }
    update(update) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = buildCheckboxDecorations(update.view);
      }
    }
  },
  {
    decorations: (v) => v.decorations,
    eventHandlers: {
      mousedown(event, view) {
        const target = event.target;
        if (!(target instanceof HTMLInputElement)) return false;
        if (!target.classList.contains("cm-checkbox-widget")) return false;
        const markFromStr = target.dataset.markFrom;
        if (!markFromStr) return false;
        const markFrom = Number(markFromStr);
        if (Number.isNaN(markFrom)) return false;
        const currentMark = view.state.sliceDoc(markFrom, markFrom + 1);
        const nextMark =
          currentMark === "x" || currentMark === "X" ? " " : "x";
        view.dispatch({
          changes: { from: markFrom, to: markFrom + 1, insert: nextMark },
        });
        event.preventDefault();
        return true;
      },
    },
  },
);

// ---------------------------------------------------------------------------
// 1.3 - Journal-mode reveal-on-select line completion timestamps (#344)
// ---------------------------------------------------------------------------
//
// Every line you finish in a journal is silently stamped with the wall-clock
// time you left it. The stamp lives IN the text (Option A) as a concealed line
// PREFIX token `⟦YYYY-MM-DD HH:MM TZ⟧` — distinctive math brackets so it can't
// collide with anything a user types, a full date + 24h time + tz captured once
// at finish. A ViewPlugin hides the token on every line and, on the ACTIVE line
// only, reveals a reformatted 12h time floated to the right. The stamp FREEZES
// at first finish: going back to fix a typo never moves the time (a line that
// already carries a token is never re-stamped). Cross-day is handled by the
// stored date: when a line was finished on a different day than the journal it
// belongs to, the reveal carries that day ("Aug 24 · 7:12 AM EDT") instead of a
// bare time — for the nights the entry is closed out the next morning.

function pad2(n) {
  return n < 10 ? "0" + n : "" + n;
}

const TS_OPEN = "⟦"; // ⟦
const TS_CLOSE = "⟧"; // ⟧
// A token is only ever a line PREFIX: ⟦YYYY-MM-DD HH:MM TZ⟧ glued to the content
// (no trailing space — concealing it to nothing leaves exactly the user's text).
// The tz group is anything up to the closing bracket, and optional (some envs
// return no zone abbreviation).
const TS_TOKEN_RE =
  /^⟦(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2})(?: ([^⟧]+))?⟧/;

const TS_MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

// Best-effort short zone abbreviation (EDT, PST, …) for a Date. Empty when the
// runtime won't give one; the token/format code both tolerate a missing tz.
function tzAbbrev(d) {
  try {
    const parts = new Intl.DateTimeFormat("en-US", {
      timeZoneName: "short",
    }).formatToParts(d);
    const tz = parts.find((p) => p.type === "timeZoneName");
    if (tz && tz.value && !/^GMT[+-]/.test(tz.value)) return tz.value;
  } catch (_) {
    /* fall through to no-tz */
  }
  return "";
}

// Build the concealed prefix token for a finish time (default: now).
function makeTimestampToken(d) {
  d = d || new Date();
  const date =
    d.getFullYear() + "-" + pad2(d.getMonth() + 1) + "-" + pad2(d.getDate());
  const time = pad2(d.getHours()) + ":" + pad2(d.getMinutes());
  const tz = tzAbbrev(d);
  const core = tz ? date + " " + time + " " + tz : date + " " + time;
  return TS_OPEN + core + TS_CLOSE;
}

/**
 * Pure: turn a stored token (or a line that starts with one) into its revealed
 * label, given the journal's entry date (YYYY-MM-DD). Same-day → bare 12h time
 * `7:12 AM EDT`; a line finished on another day → date-qualified
 * `Aug 24 · 7:12 AM EDT`. Returns "" when the string carries no valid token.
 * Exposed on `window` for unit verification.
 * @param {string} token
 * @param {string} entryDate
 * @returns {string}
 */
function formatRevealTime(token, entryDate) {
  const m = String(token || "").match(TS_TOKEN_RE);
  if (!m) return "";
  const dateStr = m[1];
  const timeStr = m[2];
  const tz = m[3] || "";
  const hh = parseInt(timeStr.slice(0, 2), 10);
  const mm = timeStr.slice(3, 5);
  const ampm = hh < 12 ? "AM" : "PM";
  let h12 = hh % 12;
  if (h12 === 0) h12 = 12;
  const clock = h12 + ":" + mm + " " + ampm + (tz ? " " + tz : "");
  if (entryDate && dateStr === entryDate) return clock;
  const mo = TS_MONTHS[parseInt(dateStr.slice(5, 7), 10) - 1] || dateStr.slice(5, 7);
  const day = parseInt(dateStr.slice(8, 10), 10);
  return mo + " " + day + " · " + clock;
}
window.formatRevealTime = formatRevealTime;

// Reveal widget: the human time, floated right on the active line only.
class RevealWidget extends WidgetType {
  constructor(label) {
    super();
    this.label = label;
  }
  eq(other) {
    return other.label === this.label;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-ts-reveal";
    span.textContent = this.label;
    return span;
  }
  ignoreEvent() {
    return true;
  }
}

// Build both the conceal decorations (hide the token on every visible line) and,
// on the active line, the end-of-line reveal widget. The conceal set is returned
// separately so it can also feed `atomicRanges` — arrow keys / Home then step
// over the hidden token instead of parking an invisible caret inside it.
function buildTimestampDecorations(view, entryDate) {
  const deco = new RangeSetBuilder();
  const atomic = new RangeSetBuilder();
  const activeLine = view.state.doc.lineAt(view.state.selection.main.head).number;
  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = view.state.doc.lineAt(pos);
      const m = line.text.match(TS_TOKEN_RE);
      if (m) {
        const tokEnd = line.from + m[0].length;
        deco.add(line.from, tokEnd, Decoration.replace({}));
        atomic.add(line.from, tokEnd, Decoration.replace({}));
        if (line.number === activeLine) {
          const label = formatRevealTime(m[0], entryDate);
          if (label) {
            deco.add(
              line.to,
              line.to,
              Decoration.widget({ widget: new RevealWidget(label), side: 1 }),
            );
          }
        }
      }
      if (line.to >= to) break;
      pos = line.to + 1;
    }
  }
  return { deco: deco.finish(), atomic: atomic.finish() };
}

// Conceal + reveal plugin, parameterized by the journal's entry date so the
// reveal can decide same-day vs cross-day. Rebuilds on doc, viewport, AND
// selection change (the reveal follows the caret to the active line).
function timestampViewPlugin(entryDate) {
  const plugin = ViewPlugin.fromClass(
    class {
      constructor(view) {
        const built = buildTimestampDecorations(view, entryDate);
        this.decorations = built.deco;
        this.atomic = built.atomic;
      }
      update(u) {
        if (u.docChanged || u.viewportChanged || u.selectionSet) {
          const built = buildTimestampDecorations(u.view, entryDate);
          this.decorations = built.deco;
          this.atomic = built.atomic;
        }
      }
    },
    {
      decorations: (v) => v.decorations,
      provide: (plugin) =>
        EditorView.atomicRanges.of(
          (view) => view.plugin(plugin)?.atomic || Decoration.none,
        ),
    },
  );
  return plugin;
}

// Stamp transactions carry this annotation so the stamp listener ignores its own
// insertions (they must not re-mark the line as freshly touched).
const stampAnnotation = Annotation.define();

// The stamp side: watch the caret leave a line the user changed, and prepend a
// completion token to it. `entryDate` is captured only to keep the whole feature
// scoped to one editor session; the closure state (touched lines + last caret
// line) is per-editor. Returns an updateListener extension plus a `flush(view)`
// used by blur/teardown to stamp the final in-progress line.
function timestampStamper() {
  // Line numbers (1-based) the user has edited this session and that are still
  // candidates for stamping. Remapped through every doc change so the identity
  // survives inserts/deletes above them.
  const touched = new Set();
  let caretLine = null;

  // Synchronous stamp of one line, honoring every guard: it must be a candidate
  // (touched this session), not already frozen (no existing token), and have
  // real content. Deleting from `touched` here makes a second call idempotent,
  // so blur + teardown can't double-stamp the same in-progress line.
  function stampLine(view, lineNo) {
    if (!view || lineNo == null) return;
    const doc = view.state.doc;
    if (lineNo < 1 || lineNo > doc.lines) return;
    if (!touched.has(lineNo)) return;
    const line = doc.line(lineNo);
    if (TS_TOKEN_RE.test(line.text)) {
      touched.delete(lineNo); // already frozen
      return;
    }
    if (line.text.trim().length === 0) return; // nothing to stamp yet; keep candidate
    touched.delete(lineNo);
    view.dispatch({
      changes: { from: line.from, to: line.from, insert: makeTimestampToken() },
      annotations: stampAnnotation.of(true),
    });
  }

  const listener = EditorView.updateListener.of((update) => {
    // Ignore our own stamp insertions: only keep the caret line in sync.
    if (update.transactions.some((tr) => tr.annotation(stampAnnotation))) {
      caretLine = update.state.doc.lineAt(
        update.state.selection.main.head,
      ).number;
      return;
    }

    // Programmatic content replacement (initial load / live sync-refresh) sets
    // `suppressDirty`. It must NEVER mark lines as user-touched, or merely
    // navigating a freshly-loaded old journal would back-stamp its lines.
    // Drop any stale candidates (the doc was replaced wholesale) and bail.
    if (suppressDirty) {
      touched.clear();
      caretLine = update.state.doc.lineAt(
        update.state.selection.main.head,
      ).number;
      return;
    }

    // Map the previously-tracked caret line forward through this edit, so
    // "which line did the caret leave" is computed against the new doc.
    if (update.docChanged) {
      const oldDoc = update.startState.doc;
      const newDoc = update.state.doc;
      if (caretLine != null && caretLine >= 1 && caretLine <= oldDoc.lines) {
        const mapped = update.changes.mapPos(oldDoc.line(caretLine).from, 1);
        caretLine = newDoc.lineAt(mapped).number;
      }
      // Remap the touched set forward, then mark the line the caret now sits on
      // as authored-this-session. Marking the CARET line (not the raw changed
      // range) is deliberate: pressing Enter at the end of a pre-existing line
      // technically "changes" that upper line (its newline), but the caret has
      // already moved to the new line below — so a plain Enter after old content
      // never marks it, and only lines the user actually composes on become
      // stamp candidates. (Under-marking a multi-line paste's interior lines is
      // the acceptable trade: a missed stamp beats stamping untouched history.)
      const remapped = new Set();
      touched.forEach((ln) => {
        if (ln < 1 || ln > oldDoc.lines) return;
        const mapped = update.changes.mapPos(oldDoc.line(ln).from, 1);
        remapped.add(newDoc.lineAt(mapped).number);
      });
      remapped.add(newDoc.lineAt(update.state.selection.main.head).number);
      touched.clear();
      remapped.forEach((n) => touched.add(n));
    }

    const newCaretLine = update.state.doc.lineAt(
      update.state.selection.main.head,
    ).number;
    if (caretLine != null && newCaretLine !== caretLine) {
      // Defer the stamp out of the update cycle (CM forbids dispatching from
      // within update). A microtask runs before the user can edit again, so the
      // captured line number is still valid when stampLine re-reads it.
      const leftLine = caretLine;
      Promise.resolve().then(() => stampLine(editorView, leftLine));
    }
    caretLine = newCaretLine;
  });

  // Blur / teardown: stamp the line the caret is currently sitting on (the last
  // line still in progress) — the reliable "finished" signal for a final line
  // the user never pressed Enter after. Synchronous so onChange fires (and the
  // token persists) before the editor is torn down.
  const blurHandler = EditorView.domEventHandlers({
    blur(_event, view) {
      stampLine(view, view.state.doc.lineAt(view.state.selection.main.head).number);
    },
  });

  function flush(view) {
    if (!view) return;
    stampLine(view, view.state.doc.lineAt(view.state.selection.main.head).number);
  }

  return { extensions: [listener, blurHandler], flush };
}

// ---------------------------------------------------------------------------
// 1.10 - Keep the caret above the soft keyboard
// ---------------------------------------------------------------------------
//
// On Android (edge-to-edge) the WebView does NOT resize when the keyboard
// opens — it overlays the bottom. So the layout viewport still reports full
// height and CodeMirror's own scrollIntoView believes the caret is visible
// when it's actually behind the keyboard. The visualViewport API *does* shrink
// to exclude the keyboard, so we use it to detect the occluded region and nudge
// the page scroller until the caret clears it. The `--keyboard-inset-bottom`
// padding (set by InsetBridge.kt) guarantees there's scroll room to do so.

function findScrollParent(el) {
  // Nearest ancestor that *can* scroll vertically. We deliberately don't gate
  // on `scrollHeight > clientHeight`: when the keyboard opens, the keyboard-
  // inset padding (which makes the container scrollable) and this lookup can
  // race, and setting `scrollTop` on a not-yet-overflowing element is a safe
  // no-op that the browser clamps. The editor's first such ancestor is the
  // page's main content column (`body` itself is `overflow: hidden`).
  let node = el ? el.parentElement : null;
  while (node) {
    const oy = getComputedStyle(node).overflowY;
    if (oy === "auto" || oy === "scroll") {
      return node;
    }
    node = node.parentElement;
  }
  return null;
}

// How much the keyboard occludes from the bottom, in CSS px.
//
// Prefer `--keyboard-inset-bottom` (set natively by InsetBridge.kt from the
// Android IME inset): on Android edge-to-edge the layout/visual viewport does
// NOT shrink when the IME opens, so `visualViewport.height` stays full and
// can't reveal the occluded region — but the native inset can. Fall back to
// visualViewport for desktop browsers / iOS, where it does shrink.
function keyboardInsetPx() {
  const v = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(
      "--keyboard-inset-bottom",
    ),
  );
  if (Number.isFinite(v) && v > 0) return v;
  const vv = window.visualViewport;
  if (vv) {
    const occluded = window.innerHeight - (vv.offsetTop + vv.height);
    if (occluded > 1) return occluded;
  }
  return 0;
}

let keepCaretQueued = false;
function keepCaretAboveKeyboard() {
  // Coalesce bursts (a keystroke fires doc + selection updates) into one rAF.
  if (keepCaretQueued) return;
  keepCaretQueued = true;
  requestAnimationFrame(() => {
    keepCaretQueued = false;
    if (!editorView || !editorView.hasFocus) return;
    const kb = keyboardInsetPx();
    if (kb <= 0) return; // keyboard hidden -> nothing to do
    const head = editorView.state.selection.main.head;
    const coords = editorView.coordsAtPos(head);
    if (!coords) return;
    const margin = 24;
    const visibleBottom = window.innerHeight - kb;
    const overflow = coords.bottom - (visibleBottom - margin);
    if (overflow > 0) {
      const scroller = findScrollParent(editorView.dom);
      if (scroller) scroller.scrollTop += overflow;
      else window.scrollBy(0, overflow);
    }
  });
}

if (window.visualViewport) {
  // Where the viewport does shrink, keyboard show/hide + pans land here.
  window.visualViewport.addEventListener("resize", keepCaretAboveKeyboard);
  window.visualViewport.addEventListener("scroll", keepCaretAboveKeyboard);
}

// On Android edge-to-edge the visual viewport does NOT shrink for the IME, so the
// resize/scroll listeners above never fire when the keyboard opens. The native
// InsetBridge dispatches this event right after it updates --keyboard-inset-bottom,
// which is the only reliable "keyboard moved" signal on that platform.
window.addEventListener("omni:keyboardinset", keepCaretAboveKeyboard);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Create a CodeMirror 6 editor instance.
 * @param {string} elementId - DOM element ID to mount the editor in
 * @param {string} initialContent - Initial document content
 * @param {Function|null} onChange - Optional callback invoked with new content string on every change
 * @param {{journalMode?: boolean, readOnly?: boolean}} [options] - Extension flags
 */
window.createEditor = function (elementId, initialContent, onChange, options) {
  // Destroy any existing editor first
  if (editorView) {
    // #344: flush the final in-progress line of the OUTGOING editor before we
    // replace it, so its completion time isn't lost on a remount.
    if (timestampFlush) {
      try {
        timestampFlush(editorView);
      } catch (e) {
        console.error("timestamp flush on recreate threw:", e);
      }
    }
    editorView.destroy();
    editorView = null;
  }
  timestampFlush = null;

  // Reset dirty state on fresh editor creation. A remount (navigate away + back)
  // makes a fresh editor, so `everDirty` clears too — the load path re-seeds from
  // the backend there, so live-refresh is allowed to resume for the new session.
  isDirty = false;
  everDirty = false;

  const parent = document.getElementById(elementId);
  if (!parent) {
    console.error("Editor container not found:", elementId);
    return;
  }

  const journalMode = !!(options && options.journalMode);
  const readOnly = !!(options && options.readOnly);
  // #344: the journal day being edited (YYYY-MM-DD), used to decide same-day vs
  // cross-day reveal formatting. Absent for notes.
  const entryDate =
    options && typeof options.entryDate === "string" ? options.entryDate : "";
  // 1.8b position restoration: a saved caret offset to restore, and a callback
  // fired whenever the selection moves so the Rust side can keep the stored
  // offset current.
  const onCursor =
    options && typeof options.onCursor === "function" ? options.onCursor : null;
  const initialCursor =
    options && Number.isFinite(options.initialCursor) ? options.initialCursor : 0;

  const extensions = [
    minimalSetup,
    markdown(),
    EditorView.lineWrapping,
    omniEditorTheme,
    autoWrapFilter,
    checkboxPlugin,
  ];

  // #344 reveal-on-select line timestamps — journal only, and never in read-only
  // (a closed journal is a frozen record; concealing tokens still applies, but no
  // stamping). The conceal/reveal plugin runs regardless so previously-stamped
  // closed entries still hide their tokens; the stamper is gated on !readOnly.
  timestampFlush = null;
  if (journalMode) {
    extensions.push(timestampViewPlugin(entryDate));
    if (!readOnly) {
      const stamper = timestampStamper();
      extensions.push(...stamper.extensions);
      timestampFlush = stamper.flush;
    }
  }

  if (readOnly) {
    // `editable.of(false)` is stronger than `EditorState.readOnly.of(true)` —
    // it disables the input cursor entirely (no caret, no focus, no selection-
    // driven edits), so the user gets a clear visual signal that typing won't
    // do anything. Used for closed journals.
    extensions.push(EditorView.editable.of(false));
  }

  // Update listener: doc changes drive onChange + dirty/clean signalling;
  // selection changes drive onCursor (1.8b) so the stored caret offset tracks
  // the live cursor even when the user only navigates (arrows / clicks) without
  // editing.
  extensions.push(
    EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        if (!suppressDirty) emitDirty();
        if (typeof onChange === "function") {
          onChange(update.state.doc.toString());
        }
      }
      if (update.selectionSet && onCursor) {
        onCursor(update.state.selection.main.head);
      }
      // Typing or moving the caret while the keyboard is up: keep it visible.
      if (update.docChanged || update.selectionSet) {
        keepCaretAboveKeyboard();
      }
    }),
  );

  editorView = new EditorView({
    state: EditorState.create({
      doc: initialContent || "",
      extensions,
    }),
    parent,
  });

  // Restore the saved caret (1.8b). `scrollIntoView` makes CodeMirror walk up to
  // the real scroll parent (the page's overflow-y-auto column — this editor has
  // no fixed height, so its own scroller never engages) and bring the line into
  // view. A selection-only dispatch isn't a doc change, so it won't flip dirty.
  if (initialCursor > 0) {
    const pos = clampCursor(initialCursor, editorView.state.doc.length);
    if (pos != null) {
      editorView.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
    }
  }
};

/**
 * Clamp a saved caret offset to a document that may have changed since it was
 * stored (e.g. the note was edited elsewhere, or restored content is shorter).
 * Returns the offset to restore, or null/undefined to skip restoration.
 * @param {number} pos - The saved caret offset.
 * @param {number} docLength - Current document length in characters.
 * @returns {number|null|undefined}
 */
function clampCursor(pos, docLength) {
  // Saved offset overflows a now-shorter doc -> drop the caret at the end
  // (keeps the user near where they were); otherwise restore it verbatim.
  return Math.min(pos, docLength);
}

/**
 * Get the current editor content.
 * @returns {string} The document content, or empty string if no editor exists
 */
window.getEditorContent = function () {
  if (!editorView) return "";
  return editorView.state.doc.toString();
};

/**
 * Get the current caret offset (selection head). Used as an unmount-time
 * fallback so a position is captured even if no selection event fired (1.8b).
 * @returns {number} The caret offset, or 0 if no editor exists.
 */
window.getEditorCursor = function () {
  if (!editorView) return 0;
  return editorView.state.selection.main.head;
};

/**
 * Replace the entire editor content. This is treated as a programmatic update
 * and does NOT flip the dirty flag - callers (e.g. after a load) can follow up
 * with window.markClean() if they need an explicit clean signal.
 * @param {string} content - New content to set
 */
window.setEditorContent = function (content) {
  if (!editorView) return;
  suppressDirty = true;
  try {
    editorView.dispatch({
      changes: {
        from: 0,
        to: editorView.state.doc.length,
        insert: content,
      },
    });
  } finally {
    suppressDirty = false;
  }
};

/**
 * Destroy the editor instance and clean up.
 */
window.destroyEditor = function () {
  if (editorView) {
    // #344: stamp the final in-progress line before tearing down, so its
    // completion time persists via the resulting onChange.
    if (timestampFlush) {
      try {
        timestampFlush(editorView);
      } catch (e) {
        console.error("timestamp flush on destroy threw:", e);
      }
    }
    editorView.destroy();
    editorView = null;
  }
  timestampFlush = null;
  emitClean();
};
