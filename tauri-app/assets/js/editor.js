import { EditorView, minimalSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";

let editorView = null;

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
// Public API
// ---------------------------------------------------------------------------

/**
 * Create a CodeMirror 6 editor instance.
 * @param {string} elementId - DOM element ID to mount the editor in
 * @param {string} initialContent - Initial document content
 * @param {Function|null} onChange - Optional callback invoked with new content string on every change
 */
window.createEditor = function (elementId, initialContent, onChange) {
  // Destroy any existing editor first
  if (editorView) {
    editorView.destroy();
    editorView = null;
  }

  const parent = document.getElementById(elementId);
  if (!parent) {
    console.error("Editor container not found:", elementId);
    return;
  }

  const extensions = [
    minimalSetup,
    markdown(),
    EditorView.lineWrapping,
    autoWrapFilter,
  ];

  // Add change listener if callback provided
  if (typeof onChange === "function") {
    extensions.push(
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          const content = update.state.doc.toString();
          onChange(content);
        }
      }),
    );
  }

  editorView = new EditorView({
    state: EditorState.create({
      doc: initialContent || "",
      extensions,
    }),
    parent,
  });
};

/**
 * Get the current editor content.
 * @returns {string} The document content, or empty string if no editor exists
 */
window.getEditorContent = function () {
  if (!editorView) return "";
  return editorView.state.doc.toString();
};

/**
 * Replace the entire editor content.
 * @param {string} content - New content to set
 */
window.setEditorContent = function (content) {
  if (!editorView) return;
  editorView.dispatch({
    changes: {
      from: 0,
      to: editorView.state.doc.length,
      insert: content,
    },
  });
};

/**
 * Destroy the editor instance and clean up.
 */
window.destroyEditor = function () {
  if (editorView) {
    editorView.destroy();
    editorView = null;
  }
};
