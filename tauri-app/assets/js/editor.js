import { EditorView, basicSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";

let editorView = null;

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

  const extensions = [basicSetup, markdown()];

  // Add change listener if callback provided
  if (typeof onChange === "function") {
    extensions.push(
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          const content = update.state.doc.toString();
          onChange(content);
        }
      })
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
