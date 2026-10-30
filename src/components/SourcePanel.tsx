import { useEffect, useRef, useState } from 'react';
import type { SourceDocument, SourcePassage, SourceView } from '../generated/contracts';
import { sourceCall, bytesToBase64, base64ToBytes } from '../sources';
import { Icon } from './Icon';

type Preview = { document: SourceDocument; passages: SourcePassage[]; raw_base64: string };
function PdfPage({ bytes, page }: { bytes: string; page: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    let dispose: (() => Promise<void>) | undefined;
    let cancel: (() => void) | undefined;
    void import('../pdf')
      .then(async ({ openPdf }) => {
        const opened = await openPdf(base64ToBytes(bytes));
        dispose = opened.destroy;
        if (!active) {
          await dispose();
          return;
        }
        const p = await opened.document.getPage(page);
        const viewport = p.getViewport({
          scale: Math.min(1.4, 1000 / p.getViewport({ scale: 1 }).width),
        });
        const element = canvas.current;
        if (!element) return;
        element.width = viewport.width;
        element.height = viewport.height;
        const render = p.render({ canvas: element, viewport });
        cancel = () => render.cancel();
        await render.promise;
      })
      .catch((e: unknown) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
      cancel?.();
      void dispose?.();
    };
  }, [bytes, page]);
  return (
    <figure>
      <canvas
        ref={canvas}
        className="pdf-page"
        aria-label={`Original PDF page ${page}. Extracted text follows below.`}
      />
      {error ? <p role="alert">Page preview unavailable: {error}</p> : null}
      <figcaption>
        Original page {page}; compare fractions, powers and reading order with the extracted text.
      </figcaption>
    </figure>
  );
}
export function SourcePanel({
  sessionId,
  activityId,
  sources,
  changed,
  initialSource,
  locked = false,
  close,
}: {
  sessionId: string;
  activityId: string;
  sources: SourceView;
  changed: () => Promise<void>;
  initialSource?: { id: string; anchor: string };
  /** A tutor or generation request is out; opening or selecting passages waits until it settles. */
  locked?: boolean;
  close?: () => void;
}) {
  const [paste, setPaste] = useState('');
  const [title, setTitle] = useState('Study passage');
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SourcePassage[]>([]);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [page, setPage] = useState(1);
  const [working, setWorking] = useState(false);
  const busy = working || locked;
  const [error, setError] = useState('');
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, [sessionId, activityId]);
  const work = async (f: () => Promise<void>) => {
    setWorking(true);
    setError('');
    try {
      await f();
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setWorking(false);
    }
  };
  const show = async (id: string, anchor?: string) => {
    const next = await sourceCall<Preview>('preview', { sessionId, sourceId: id });
    await changed();
    if (!mounted.current) return;
    setPage(anchor?.startsWith('page:') ? Number(anchor.slice(5)) : 1);
    setPreview(next);
  };
  useEffect(() => {
    if (initialSource) void work(() => show(initialSource.id, initialSource.anchor));
  }, [initialSource]);
  const importFile = async (file: File) => {
    if (file.size > 8 * 1024 * 1024) throw new Error('Choose a file of at most 8 MiB.');
    const bytes = new Uint8Array(await file.arrayBuffer());
    const pdf = file.name.toLowerCase().endsWith('.pdf');
    const kind = pdf ? 'pdf' : /\.md(?:own)?$/i.test(file.name) ? 'markdown' : 'text';
    const extracted = pdf ? await (await import('../pdf')).extractPdf(bytes) : [];
    const doc = await sourceCall<SourceDocument>('import', {
      title: file.name.slice(0, 160),
      kind,
      raw_base64: bytesToBase64(bytes),
      pages: extracted,
      extraction_version: pdf ? 'pdfjs-6.4.299-text-v1' : 'utf8-sections-v1',
    });
    await changed();
    await show(doc.id);
  };
  return (
    <aside className="visual-panel source-panel" aria-label="Sources">
      <header className="panel-head">
        <h2>Your sources</h2>
        {close ? (
          <button className="dialog-close" aria-label="Close sources" onClick={close}>
            <Icon name="close" />
          </button>
        ) : null}
      </header>
      <p className="panel-note">
        Only files you choose or text you paste are imported. Opening a passage is recorded as
        source support before it is shown, and nothing in a source can change how the tutor behaves.
      </p>
      {error ? (
        <p role="alert" className="error">
          {error}
        </p>
      ) : null}
      {working ? (
        <p role="status" className="source-busy">
          Working with the selected source…
        </p>
      ) : null}
      <div className="source-import">
        <label htmlFor="source-file">
          <Icon name="doc" />
          <span>
            Import a file
            <small>Text, Markdown or a PDF with selectable text · up to 8 MB</small>
          </span>
        </label>
        <input
          id="source-file"
          type="file"
          accept=".txt,.md,.markdown,.pdf,text/plain,text/markdown,application/pdf"
          disabled={busy}
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void work(() => importFile(file));
            e.target.value = '';
          }}
        />
      </div>
      <details className="disclosure">
        <summary>Paste source text</summary>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void work(async () => {
              const doc = await sourceCall<SourceDocument>('import', {
                title,
                kind: 'text',
                raw_base64: bytesToBase64(new TextEncoder().encode(paste)),
                pages: [],
                extraction_version: 'utf8-sections-v1',
              });
              setPaste('');
              await changed();
              await show(doc.id);
            });
          }}
        >
          <label htmlFor="source-title">Source title</label>
          <input
            id="source-title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            maxLength={160}
            required
          />
          <label htmlFor="source-paste">Source text</label>
          <textarea
            id="source-paste"
            value={paste}
            onChange={(e) => setPaste(e.target.value)}
            maxLength={200000}
            required
          />
          <button disabled={busy || !paste.trim()}>Import pasted text</button>
        </form>
      </details>
      <ul className="source-list">
        {sources.documents.map((s) => (
          <li key={s.id}>
            <button disabled={busy} onClick={() => void work(() => show(s.id))}>
              <Icon name="doc" />
              <span>{s.title}</span>
            </button>
            <span className="small">
              {s.kind}
              {s.page_count ? ` · ${s.page_count} pages` : ''}
            </span>
          </li>
        ))}
      </ul>
      <form
        className="source-search"
        onSubmit={(e) => {
          e.preventDefault();
          void work(async () => {
            const found = await sourceCall<SourcePassage[]>('search', { sessionId, query });
            await changed();
            if (mounted.current) setResults(found);
          });
        }}
      >
        <label htmlFor="source-query">Find a passage</label>
        <div className="search-row">
          <input
            id="source-query"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            maxLength={300}
          />
          <button disabled={busy || !query.trim()}>Search</button>
        </div>
      </form>
      {results.length ? (
        <ul className="source-results">
          {results.map((p) => (
            <li key={p.id}>
              <button disabled={busy} onClick={() => void work(() => show(p.source_id, p.anchor))}>
                {p.title} · {p.anchor}
              </button>
              <p>{p.text}</p>
            </li>
          ))}
        </ul>
      ) : null}
      {preview ? (
        <section className="source-preview" aria-label="Source preview">
          <h3>{preview.document.title}</h3>
          {preview.document.warnings.map((w) => (
            <p key={w} className="notice small">
              {w}
            </p>
          ))}
          <details className="file-details">
            <summary>File details</summary>
            <p className="small">
              SHA-256: <code>{preview.document.sha256}</code>
              <br />
              Extraction: {preview.document.extraction_version}
            </p>
          </details>
          {preview.document.kind === 'pdf' ? (
            <>
              <label htmlFor="source-page">Original page</label>
              <input
                id="source-page"
                type="number"
                min={1}
                max={preview.document.page_count}
                value={page}
                onChange={(e) =>
                  setPage(
                    Math.min(preview.document.page_count, Math.max(1, Number(e.target.value) || 1)),
                  )
                }
              />
              <PdfPage bytes={preview.raw_base64} page={page} />
            </>
          ) : null}
          <p className="small">
            Select up to three passages for the tutor. Lexical retrieval adds a few relevant
            matches; evaluation items are kept in a separate store.
          </p>
          {preview.passages
            .filter((p) => preview.document.kind !== 'pdf' || p.anchor === `page:${page}`)
            .map((p) => (
              <article key={p.id} className="source-passage">
                <h4>{p.anchor}</h4>
                <p className="source-text">{p.text}</p>
                <label>
                  <input
                    type="checkbox"
                    checked={sources.selected_passage_ids.includes(p.id)}
                    disabled={busy}
                    onChange={(e) =>
                      void work(async () => {
                        await sourceCall('select', {
                          sessionId,
                          chunkId: p.id,
                          selected: e.target.checked,
                        });
                        await changed();
                      })
                    }
                  />{' '}
                  Use this passage
                </label>
              </article>
            ))}
        </section>
      ) : null}
    </aside>
  );
}
