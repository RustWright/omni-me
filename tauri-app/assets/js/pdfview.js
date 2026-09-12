// PDF rendering for the document viewer.
//
// ⛔ **Renders on-device, from bytes already in hand.** The archive exists so a
// document stays viewable offline, and `core::attachments` keeps a bounded cache
// for exactly that. Rasterizing server-side would cost a round-trip per page and
// break the offline case entirely. `core::statement::pdf::rasterize_pdf` is a
// different job — it feeds the vision model during extraction, and ⛔ its page
// cap must never reach this path.
//
// ⚠️ **This exists because Android WebView has no PDF renderer.** An `<iframe>`
// pointed at a `blob:` URL works on desktop webkit and silently shows nothing on
// Android, which is the single most common document type in the corpus failing
// to display on the device most used to read it.

import * as pdfjsLib from "pdfjs-dist";

// The worker is bundled beside this file and loaded by path, not from a CDN:
// the app must work with no network, and the Tauri/Android asset origin cannot
// reach one anyway.
//
// ⚠️ Both this file and the worker are copied into BOTH targets — `web/public`
// for desktop and the Android assets dir. A copy step that updates one leaves
// the other rendering an older build, which is how a fixed bug reappears on one
// platform only.
pdfjsLib.GlobalWorkerOptions.workerSrc = "/assets/js/pdf.worker.bundle.js";

// Pages are rendered at this CSS width; the canvas backing store is multiplied
// by devicePixelRatio on top, so text stays sharp on a phone.
//
// ⚠️ Deliberately a fixed width rather than the container's: the container is
// inside a responsive grid whose width settles *after* first paint, and reading
// it during render produced a first page at the wrong scale on every load.
const RENDER_WIDTH = 900;

// ⚠️ A cap on *rendering*, not on the document. Every page is still counted and
// reported; this only bounds how many canvases exist at once, because a
// 200-page statement at ~1.5 MB of backing store per canvas will exhaust an
// Android WebView. ⛔ It is not the extraction path's 8-page budget and must not
// be conflated with it — that one is about a model's input size.
const MAX_RENDERED_PAGES = 30;

/**
 * Render a PDF into `container`.
 *
 * @param {string} containerId - element to fill; cleared first.
 * @param {string} url - a `blob:` URL owned by the caller.
 * @returns {Promise<{pages: number, rendered: number}>}
 */
window.renderPdf = async function (containerId, url) {
  const container = document.getElementById(containerId);
  if (!container) {
    throw new Error(`pdf container #${containerId} not found`);
  }
  container.replaceChildren();

  const doc = await pdfjsLib.getDocument({ url }).promise;
  const total = doc.numPages;
  const toRender = Math.min(total, MAX_RENDERED_PAGES);

  for (let n = 1; n <= toRender; n++) {
    const page = await doc.getPage(n);

    const unscaled = page.getViewport({ scale: 1 });
    const scale = RENDER_WIDTH / unscaled.width;
    const viewport = page.getViewport({ scale });
    const outputScale = window.devicePixelRatio || 1;

    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width * outputScale);
    canvas.height = Math.floor(viewport.height * outputScale);
    // The element stays responsive; only the backing store is device-scaled.
    canvas.style.width = "100%";
    canvas.style.height = "auto";
    canvas.style.display = "block";
    canvas.style.marginBottom = "8px";
    canvas.style.borderRadius = "4px";
    canvas.setAttribute("aria-label", `Page ${n} of ${total}`);
    container.appendChild(canvas);

    await page.render({
      canvasContext: canvas.getContext("2d"),
      transform:
        outputScale !== 1 ? [outputScale, 0, 0, outputScale, 0, 0] : null,
      viewport,
    }).promise;

    // Frees the page's own scratch buffers. Without this a long document holds
    // every page's operator list alive for as long as the view is open.
    page.cleanup();
  }

  // ⛔ The caller is told what was left out; it must never be silent. A document
  // that appears to end at page 30 reads as a complete document.
  return { pages: total, rendered: toRender };
};

/** Release a rendered document's canvases. Safe to call on an empty container. */
window.destroyPdf = function (containerId) {
  const container = document.getElementById(containerId);
  if (container) {
    container.replaceChildren();
  }
};
