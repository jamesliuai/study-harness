// Local PDF worker only. No PDF viewer scripting or arbitrary worker URLs.
import * as pdfjs from 'pdfjs-dist/legacy/build/pdf.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
export const PDF_EXTRACTION = 'pdfjs-6.4.299-text-v1';
export async function openPdf(bytes: Uint8Array) {
  const port = new Worker(workerUrl, { type: 'module' });
  const worker = pdfjs.PDFWorker.create({ port });
  const task = pdfjs.getDocument({
    data: bytes.slice(),
    worker,
    stopAtErrors: true,
    maxImageSize: 16000000,
    canvasMaxAreaInBytes: 64000000,
    useSystemFonts: false,
    disableAutoFetch: true,
    disableFontFace: false,
    enableXfa: false,
    useWasm: false,
    standardFontDataUrl: new URL('/pdf/standard_fonts/', location.href).href,
    cMapUrl: new URL('/pdf/cmaps/', location.href).href,
    cMapPacked: true,
  });
  const document = await task.promise.catch(async (e: unknown) => {
    await worker.destroy();
    port.terminate();
    throw e;
  });
  return {
    document,
    destroy: async () => {
      await task.destroy();
      await worker.destroy();
      port.terminate();
    },
  };
}
export async function extractPdf(bytes: Uint8Array) {
  const opened = await openPdf(bytes);
  try {
    if (opened.document.numPages > 100) throw new Error('Choose a PDF of at most 100 pages.');
    const pages = [];
    for (let page = 1; page <= opened.document.numPages; page++) {
      const current = await opened.document.getPage(page);
      const content = await current.getTextContent();
      const text = content.items
        .map((item) => ('str' in item ? item.str + (item.hasEOL ? '\n' : ' ') : ''))
        .join('');
      pages.push({ page, text });
      current.cleanup();
    }
    return pages;
  } finally {
    await opened.destroy();
  }
}
