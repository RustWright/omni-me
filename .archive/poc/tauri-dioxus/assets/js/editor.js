import { EditorView, basicSetup } from "codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";

let editorView = null;

window.createEditor = function (elementId, initialContent) {
  const parent = document.getElementById(elementId);
  if (!parent) {
    console.error("Editor container not found:", elementId);
    return;
  }

  editorView = new EditorView({
    state: EditorState.create({
      doc: initialContent || "",
      extensions: [basicSetup, markdown()],
    }),
    parent,
  });
};

window.getEditorContent = function () {
  if (!editorView) return "";
  return editorView.state.doc.toString();
};

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
