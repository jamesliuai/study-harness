import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { SourceDocument, Workspace } from '../generated/contracts';
import { bytesToBase64, sourceCall } from '../sources';
import { Icon } from './Icon';
import { Menu, MenuItem } from './Menu';

type Attached = { document: SourceDocument; excerpt: string; passages: number };

// Optional starting points across STEM. Choosing one only fills the box; nothing is preset or required.
const EXAMPLES = [
  { subject: 'Proof', text: 'Prove that √2 is irrational, and understand why the proof works' },
  { subject: 'Physics', text: 'Why does a satellite stay in orbit instead of falling?' },
  { subject: 'Chemistry', text: 'Balance redox equations in acidic solution' },
  { subject: 'Biology', text: 'How does a neuron fire an action potential?' },
  {
    subject: 'Statistics',
    text: 'When should I use a paired t-test, and how do I read the p-value?',
  },
  {
    subject: 'Algorithms',
    text: 'Trace how quicksort partitions an array and why it is O(n log n) on average',
  },
  {
    subject: 'Linear algebra',
    text: 'Find eigenvalues and eigenvectors of a 2×2 matrix and say what they mean',
  },
  { subject: 'Calculus', text: 'Why does the chain rule work, not just how to use it?' },
];
const SHOWN = 4;
// The core measures a goal in bytes (UTF-8), not characters.
const MAX_GOAL_BYTES = 4000;

/**
 * The starting point: the learner's own goal, problem or notes. Material can be attached and checked before planning;
 * the plan is drafted by the tutor model and shown for editing before anything starts.
 */
export function Home({
  workspace: w,
  busy,
  planning,
  draft,
  tools,
  onDraft,
  onPlan,
  onSample,
  onOpen,
  onReview,
  onSettings,
}: {
  workspace: Workspace;
  busy: boolean;
  planning: boolean;
  draft: string;
  /** The model and effort pickers. */
  tools?: ReactNode;
  onDraft: (text: string) => void;
  onPlan: (goal: string, sourceIds: string[]) => void;
  onSample: (objectiveId: string) => void;
  onOpen: (sessionId: string) => void;
  onReview: () => void;
  onSettings: () => void;
}) {
  const [goal, setGoal] = useState(draft);
  const [attached, setAttached] = useState<Attached[]>([]);
  const [paste, setPaste] = useState('');
  const [pasting, setPasting] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState('');
  const [offset, setOffset] = useState(0);
  const box = useRef<HTMLTextAreaElement>(null);
  const canPlan = w.offline || w.credential.configured;
  useEffect(() => {
    box.current?.focus();
  }, []);
  // Grow with the goal up to the stylesheet's max-height.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight + 2}px`;
  }, [goal]);
  const attach = async (doc: SourceDocument) => {
    if (attached.some((a) => a.document.id === doc.id)) return;
    const view = await sourceCall<Attached>('excerpt', { sourceId: doc.id });
    setAttached((list) => [...list, view].slice(0, 6));
  };
  const work = async (f: () => Promise<void>) => {
    setWorking(true);
    setError('');
    try {
      await f();
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  };
  const importFile = (file: File) =>
    work(async () => {
      if (file.size > 8 * 1024 * 1024) throw new Error('Choose a file of at most 8 MiB.');
      const bytes = new Uint8Array(await file.arrayBuffer());
      const pdf = file.name.toLowerCase().endsWith('.pdf');
      const kind = pdf ? 'pdf' : /\.md(?:own)?$/i.test(file.name) ? 'markdown' : 'text';
      const pages = pdf ? await (await import('../pdf')).extractPdf(bytes) : [];
      const doc = await sourceCall<SourceDocument>('import', {
        title: file.name.slice(0, 160),
        kind,
        raw_base64: bytesToBase64(bytes),
        pages,
        extraction_version: pdf ? 'pdfjs-6.4.299-text-v1' : 'utf8-sections-v1',
      });
      await attach(doc);
    });
  const importPaste = () =>
    work(async () => {
      const first = paste
        .trim()
        .split('\n')[0]
        .replace(/^#+\s*/, '')
        .slice(0, 60);
      const doc = await sourceCall<SourceDocument>('import', {
        title: `Pasted notes · ${first}`,
        kind: 'text',
        raw_base64: bytesToBase64(new TextEncoder().encode(paste)),
        pages: [],
        extraction_version: 'utf8-sections-v1',
      });
      await attach(doc);
      setPaste('');
      setPasting(false);
    });
  const library = w.sources.documents.filter((d) => !attached.some((a) => a.document.id === d.id));
  const recent = w.goals.slice(0, 4);
  const due = w.unit.due_review_count;
  const disabled = busy || working || planning;
  const goalBytes = new TextEncoder().encode(goal).length;
  const tooLong = goalBytes > MAX_GOAL_BYTES;
  const ready = !!goal.trim() && !tooLong && canPlan && !disabled;
  const submit = () => {
    if (ready)
      onPlan(
        goal.trim(),
        attached.map((a) => a.document.id),
      );
  };
  const examples = Array.from(
    { length: SHOWN },
    (_, i) => EXAMPLES[(offset + i) % EXAMPLES.length],
  );
  return (
    <div className="activity-inner home">
      <form
        className="goal-start"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label htmlFor="goal" className="home-title">
          What would you like to understand or be able to do?
        </label>
        <p className="home-lead">
          A topic, a question, a problem you are stuck on, or your own notes. You get a short plan
          to look over and change before anything starts.
        </p>
        <div className="composer-pill goal-pill" data-disabled={disabled || undefined}>
          <textarea
            id="goal"
            ref={box}
            rows={3}
            value={goal}
            maxLength={MAX_GOAL_BYTES}
            disabled={disabled}
            placeholder="e.g. “I keep mixing up when to use conservation of energy versus momentum” — or paste the problem itself"
            onChange={(e) => {
              setGoal(e.target.value);
              onDraft(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                submit();
              }
            }}
          />
          {attached.length ? (
            <ul className="attached" aria-label="Material for this goal">
              {attached.map((a) => (
                <li key={a.document.id}>
                  <div className="attached-head">
                    <Icon name="doc" />
                    <span className="attached-title">{a.document.title}</span>
                    <span className="attached-meta">
                      {a.document.kind === 'pdf' ? `${a.document.page_count} pages · ` : ''}
                      {a.passages} passage{a.passages === 1 ? '' : 's'}
                    </span>
                    <button
                      type="button"
                      className="icon-button"
                      disabled={disabled}
                      aria-label={`Remove ${a.document.title}`}
                      title="Remove"
                      onClick={() =>
                        setAttached((list) => list.filter((x) => x.document.id !== a.document.id))
                      }
                    >
                      <Icon name="close" />
                    </button>
                  </div>
                  {a.document.warnings.map((warning) => (
                    <p key={warning} className="notice notice-warn small">
                      {warning}
                    </p>
                  ))}
                  <details>
                    <summary>What was read from it</summary>
                    <p className="source-text">{a.excerpt || 'No text could be extracted.'}</p>
                    <p className="small muted">
                      If equations, tables or pages came out wrong, paste a corrected version as
                      notes instead — the plan only sees this extracted text.
                    </p>
                  </details>
                </li>
              ))}
            </ul>
          ) : null}
          {pasting ? (
            <div className="paste-notes">
              <label htmlFor="paste-notes">Paste notes or a problem set</label>
              <textarea
                id="paste-notes"
                value={paste}
                maxLength={200000}
                disabled={disabled}
                onChange={(e) => setPaste(e.target.value)}
              />
              <div className="actions">
                <button
                  type="button"
                  disabled={disabled || !paste.trim()}
                  onClick={() => void importPaste()}
                >
                  Attach these notes
                </button>
                <button type="button" className="text-button" onClick={() => setPasting(false)}>
                  Cancel
                </button>
              </div>
            </div>
          ) : null}
          <div className="composer-bar">
            <label className="ghost file-button" data-disabled={disabled || undefined}>
              <Icon name="clip" />
              <span>Attach</span>
              <input
                type="file"
                accept=".txt,.md,.markdown,.pdf,text/plain,text/markdown,application/pdf"
                disabled={disabled}
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) void importFile(file);
                  e.target.value = '';
                }}
              />
            </label>
            {!pasting ? (
              <button
                type="button"
                className="ghost"
                disabled={disabled}
                onClick={() => setPasting(true)}
              >
                <Icon name="clipboard" />
                <span>Paste notes</span>
              </button>
            ) : null}
            {library.length ? (
              <Menu
                className="ghost-menu"
                icon="folder"
                label="Your sources"
                title="Attach something you imported before"
                disabled={disabled}
                placement="bottom"
                panelClassName="source-menu"
              >
                {(close) =>
                  library.map((d) => (
                    <MenuItem
                      key={d.id}
                      icon="doc"
                      label={d.title}
                      detail={d.kind === 'pdf' ? `PDF · ${d.page_count} pages` : d.kind}
                      onSelect={() => {
                        close();
                        void work(() => attach(d));
                      }}
                    />
                  ))
                }
              </Menu>
            ) : null}
            <span className="composer-spacer" />
            {tools}
            <button
              type="submit"
              className="send"
              disabled={!ready}
              aria-label={planning ? 'Drafting a plan…' : 'Make a plan'}
            >
              <span>{planning ? 'Drafting a plan…' : 'Make a plan'}</span>
              <Icon name="arrow" />
            </button>
          </div>
        </div>
        <div className="composer-tray">
          {tooLong ? (
            <span className="tray-warn" role="status">
              {(goalBytes - MAX_GOAL_BYTES).toLocaleString()} bytes too long for a goal — attach the
              rest as notes
            </span>
          ) : working ? (
            <span className="tray-muted" role="status">
              <span className="spinner" aria-hidden="true" />
              Reading the material…
            </span>
          ) : canPlan ? (
            <span className="tray-muted">
              {w.offline
                ? 'Offline development · plans and tasks are simulated fixtures'
                : 'Your goal and a few attached passages go to OpenAI to draft the plan · everything is stored on this Mac'}
            </span>
          ) : (
            <button type="button" className="tray-link" onClick={onSettings}>
              <Icon name="key" />
              Add your OpenAI key to plan new topics
            </button>
          )}
          <span className="composer-spacer" />
          {canPlan ? (
            <span className="tray-muted kbd-hint">
              <kbd>⌘</kbd>
              <kbd>↩</kbd> plan
            </span>
          ) : (
            <span className="tray-muted">The sample unit below works without a key</span>
          )}
        </div>
        {error ? (
          <p className="error small" role="alert">
            <Icon name="alert" />
            <span>{error}</span>
          </p>
        ) : null}
      </form>
      {!goal.trim() ? (
        <section className="examples" aria-label="Example goals">
          <div className="section-head">
            <h2>Ideas to start from</h2>
            <button
              type="button"
              className="text-button"
              onClick={() => setOffset((o) => (o + SHOWN) % EXAMPLES.length)}
            >
              <Icon name="shuffle" />
              More ideas
            </button>
          </div>
          <div className="example-grid">
            {examples.map((example) => (
              <button
                key={example.text}
                type="button"
                className="example"
                disabled={disabled}
                onClick={() => {
                  setGoal(example.text);
                  onDraft(example.text);
                  box.current?.focus();
                }}
              >
                <span className="example-subject">{example.subject}</span>
                <span className="example-text">{example.text}</span>
              </button>
            ))}
          </div>
        </section>
      ) : null}
      {due || recent.length ? (
        <section className="home-section" aria-label="Pick up where you left off">
          <div className="section-head">
            <h2>Pick up where you left off</h2>
          </div>
          {due ? (
            <button className="home-row" data-kind="review" disabled={disabled} onClick={onReview}>
              <span className="home-row-icon">
                <Icon name="review" />
              </span>
              <span className="home-row-text">
                Review for a few minutes
                <small>
                  {due} later check{due === 1 ? '' : 's'} ready · a new problem each time; overdue
                  means ready, not failed
                </small>
              </span>
              <Icon name="arrow" />
            </button>
          ) : null}
          {recent.map((g) => (
            <button
              key={g.id}
              className="home-row"
              disabled={disabled || !g.session_ids.length}
              onClick={() => onOpen(g.session_ids[g.session_ids.length - 1])}
            >
              <span className="home-row-icon">
                <Icon name="list" />
              </span>
              <span className="home-row-text">
                {g.title}
                <small>
                  {g.capabilities} capabilit{g.capabilities === 1 ? 'y' : 'ies'} ·{' '}
                  {g.checked ? `${g.checked} shown on your own` : 'nothing checked yet'} · started{' '}
                  {new Date(g.created_at).toLocaleDateString([], {
                    month: 'short',
                    day: 'numeric',
                  })}
                </small>
              </span>
              {g.capabilities ? (
                <span className="row-meter" aria-hidden="true">
                  <span style={{ width: `${(100 * g.checked) / g.capabilities}%` }} />
                </span>
              ) : null}
              <Icon name="arrow" />
            </button>
          ))}
        </section>
      ) : null}
      <details className="home-section sample-unit">
        <summary>
          <span className="home-row-icon">
            <Icon name="book" />
          </span>
          <span className="home-row-text">
            Try the sample unit
            <small>
              Gradient descent and convergence · {w.unit.objectives.length} capabilities · works
              without a key
            </small>
          </span>
        </summary>
        <p className="small muted">{w.unit.notice}</p>
        <ol className="sample-list">
          {w.unit.objectives.map((o) => (
            <li key={o.id}>
              <span className="objective-outcome">{o.outcome}</span>
              <button className="ghost" disabled={disabled} onClick={() => onSample(o.id)}>
                Start
                <Icon name="arrow" />
              </button>
            </li>
          ))}
        </ol>
      </details>
    </div>
  );
}
