import { EditorView, minimalSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, RangeSetBuilder } from "@codemirror/state";
import { Decoration, ViewPlugin, WidgetType, keymap } from "@codemirror/view";

let editorView = null;
let isDirty = false;
let suppressDirty = false;
const dirtyListeners = [];
const cleanListeners = [];

// ---------------------------------------------------------------------------
// 1.4 - Dirty / Clean signalling
// ---------------------------------------------------------------------------

function emitDirty() {
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
// 1.3 - Journal-mode line timestamp on Enter
// ---------------------------------------------------------------------------

function pad2(n) {
  return n < 10 ? "0" + n : "" + n;
}

function currentTimestamp() {
  const d = new Date();
  return pad2(d.getHours()) + ":" + pad2(d.getMinutes()) + " ";
}

// Keymap entry: on Enter at end of line, insert newline + HH:MM + space.
// If the user pressed Enter mid-line, behave normally (don't inject timestamp).
function timestampEnterHandler(view) {
  const { state } = view;
  const sel = state.selection.main;
  if (!sel.empty) return false;
  const line = state.doc.lineAt(sel.from);
  if (sel.from !== line.to) return false; // not at end of line
  const ts = currentTimestamp();
  view.dispatch({
    changes: { from: sel.from, to: sel.from, insert: "\n" + ts },
    selection: { anchor: sel.from + 1 + ts.length },
    userEvent: "input",
    scrollIntoView: true,
  });
  return true;
}

const journalTimestampKeymap = keymap.of([
  { key: "Enter", run: timestampEnterHandler },
]);

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
    editorView.destroy();
    editorView = null;
  }

  // Reset dirty state on fresh editor creation.
  isDirty = false;

  const parent = document.getElementById(elementId);
  if (!parent) {
    console.error("Editor container not found:", elementId);
    return;
  }

  const journalMode = !!(options && options.journalMode);
  const readOnly = !!(options && options.readOnly);
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
    autoWrapFilter,
    checkboxPlugin,
  ];

  if (journalMode) {
    // Prepend timestamp keymap so it runs before minimalSetup's Enter handler.
    extensions.unshift(journalTimestampKeymap);
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
    editorView.destroy();
    editorView = null;
  }
  emitClean();
};
