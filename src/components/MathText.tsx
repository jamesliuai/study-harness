import { Fragment, useEffect, useRef } from 'react';
import type { ReactNode } from 'react';

// Conservative KaTeX settings shared by every math span: no trust, strict parsing, bounded expansion and size.
const KATEX = {
  throwOnError: true,
  trust: false,
  strict: 'error',
  maxExpand: 100,
  maxSize: 20,
  output: 'htmlAndMathml',
} as const;
// Bounds on rendering work for one block of text; anything beyond stays plain, escaped text.
const MAX_TEXT = 12000;
const MAX_MATH = 40;
const MAX_TEX = 2000;

export function MathText({ tex, block = false }: { tex: string; block?: boolean }) {
  const target = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    let active = true;
    if (target.current) target.current.textContent = tex;
    void import('katex')
      .then(({ default: katex }) => {
        if (!active || !target.current) return;
        try {
          katex.render(tex.slice(0, MAX_TEX), target.current, { ...KATEX, displayMode: block });
        } catch {
          // Unsupported or malformed TeX keeps its source text as the accessible fallback.
          target.current.textContent = tex;
        }
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [tex, block]);
  return <span className={block ? 'math-block' : 'math-inline'} ref={target} />;
}

type Piece = { kind: 'text' | 'inline' | 'display'; value: string };

/**
 * Splits prose into text and TeX pieces: `$$…$$` is display math and `$…$` inline math when the delimiters hug
 * non-space characters and the closing `$` is not followed by a digit (so “$5 and $10” stays text). `\$` is a
 * literal dollar sign. Text pieces are rendered as React text (escaped); nothing is interpreted as HTML.
 */
export function splitMath(text: string): Piece[] {
  const pieces: Piece[] = [];
  let buffer = '';
  let math = 0;
  let i = 0;
  const flush = () => {
    if (buffer) pieces.push({ kind: 'text', value: buffer });
    buffer = '';
  };
  while (i < text.length) {
    const ch = text[i];
    if (ch === '\\' && text[i + 1] === '$') {
      buffer += '$';
      i += 2;
      continue;
    }
    if (ch === '$' && math < MAX_MATH) {
      if (text[i + 1] === '$') {
        const end = text.indexOf('$$', i + 2);
        if (end > i + 2) {
          flush();
          pieces.push({ kind: 'display', value: text.slice(i + 2, end).trim() });
          math++;
          i = end + 2;
          continue;
        }
      } else if (text[i + 1] && !/\s/.test(text[i + 1])) {
        let end = i + 1;
        while (end < text.length && (text[end] !== '$' || text[end - 1] === '\\')) {
          if (text[end] === '\n') break;
          end++;
        }
        if (
          text[end] === '$' &&
          !/\s/.test(text[end - 1]) &&
          !/\d/.test(text[end + 1] ?? '') &&
          end - i - 1 <= MAX_TEX
        ) {
          flush();
          pieces.push({ kind: 'inline', value: text.slice(i + 1, end) });
          math++;
          i = end + 1;
          continue;
        }
      }
    }
    buffer += ch;
    i++;
  }
  flush();
  return pieces;
}

/** `**bold**` and `code` spans in plain text; an unmatched marker stays literal. Never interprets HTML. */
function Emphasis({ text, bold }: { text: string; bold: boolean }) {
  const pieces = text.split(/(`[^`\n]+`)/);
  const out = pieces.map((piece, i) =>
    /^`[^`]+`$/.test(piece) ? (
      <code key={i}>{piece.slice(1, -1)}</code>
    ) : (
      <Fragment key={i}>{piece}</Fragment>
    ),
  );
  return bold ? <strong>{out}</strong> : <>{out}</>;
}

function Inline({ text }: { text: string }) {
  const pieces = splitMath(text);
  // Bold may span typeset maths (“**the value $x$ is**”), so markers are counted across text pieces.
  const markers = pieces
    .filter((p) => p.kind === 'text')
    .reduce((n, p) => n + p.value.split('**').length - 1, 0);
  const emphasis = markers > 0 && markers % 2 === 0;
  const out: ReactNode[] = [];
  let bold = false;
  pieces.forEach((p, i) => {
    if (p.kind !== 'text') {
      const node = <MathText key={i} tex={p.value} block={p.kind === 'display'} />;
      out.push(bold ? <strong key={i}>{node}</strong> : node);
      return;
    }
    (emphasis ? p.value.split('**') : [p.value]).forEach((segment, j) => {
      if (j > 0) bold = !bold;
      if (segment) out.push(<Emphasis key={`${i}-${j}`} text={segment} bold={bold} />);
    });
  });
  return <>{out}</>;
}

const isStep = (l: string) => /^\s*\d+[.)]\s+/.test(l);
const isBullet = (l: string) => /^\s*[-*•]\s+/.test(l);

/** Lines that form a list (with an optional lead line), or null when the block is ordinary prose. */
function listOf(lines: string[], test: (l: string) => boolean) {
  const items = lines.filter(test);
  if (items.length < 2 || items.length < lines.length - 1) return null;
  const first = lines.findIndex(test);
  const lead = lines.slice(0, first);
  const rows: string[][] = [];
  // Lines keep their order: a line after an item continues that item.
  for (const l of lines.slice(first)) {
    if (test(l)) rows.push([l.replace(/^\s*(?:\d+[.)]|[-*•])\s+/, '')]);
    else rows[rows.length - 1].push(l.trim());
  }
  return { lead, rows, start: Number(/\d+/.exec(items[0])?.[0] ?? 1) || undefined };
}

function Rows({ rows }: { rows: string[][] }) {
  return (
    <>
      {rows.map((item, i) => (
        <li key={i}>
          {item.map((l, j) => (
            <Fragment key={j}>
              {j ? <br /> : null}
              <Inline text={l} />
            </Fragment>
          ))}
        </li>
      ))}
    </>
  );
}

function Blocks({ text, keyPrefix }: { text: string; keyPrefix: string }) {
  const blocks = text.split(/\n\s*\n/).filter((b) => b.trim());
  const out: ReactNode[] = [];
  blocks.forEach((block, b) => {
    const key = `${keyPrefix}-${b}`;
    const lines = block.split('\n').filter((l) => l.trim());
    const steps = listOf(lines, isStep);
    const bullets = steps ? null : listOf(lines, isBullet);
    const list = steps ?? bullets;
    if (list) {
      if (list.lead.length)
        out.push(
          <p key={`${key}-lead`}>
            <Inline text={list.lead.join(' ')} />
          </p>,
        );
      out.push(
        steps ? (
          <ol key={key} className="rich-steps" start={list.start}>
            <Rows rows={list.rows} />
          </ol>
        ) : (
          <ul key={key} className="rich-list">
            <Rows rows={list.rows} />
          </ul>
        ),
      );
      return;
    }
    const heading = lines.length === 1 && /^\s*#{1,4}\s+\S/.test(lines[0]);
    if (heading) {
      out.push(
        <p key={key} className="rich-heading">
          <Inline text={lines[0].replace(/^\s*#{1,4}\s+/, '')} />
        </p>,
      );
      return;
    }
    const display = /^\s*\$\$[\s\S]*\$\$\s*$/.test(block);
    out.push(
      display ? (
        <div key={key} className="rich-display">
          <Inline text={block.trim()} />
        </div>
      ) : (
        <p key={key}>
          {lines.map((l, i) => (
            <Fragment key={i}>
              {i ? <br /> : null}
              <Inline text={l} />
            </Fragment>
          ))}
        </p>
      ),
    );
  });
  return <>{out}</>;
}

/**
 * Bounded, escaped prose with mathematics: paragraphs split on blank lines, single line breaks kept, numbered steps
 * and bullets rendered as lists, `**bold**`, `code` and fenced code blocks, `$…$`/`$$…$$` typeset with KaTeX. Used
 * for tasks, learner text and tutor replies alike. Never renders HTML from its input.
 */
export function RichText({ text, className }: { text: string; className?: string }) {
  const bounded = text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}…` : text;
  const out: ReactNode[] = [];
  const fence = /^```[^\n]*\n([\s\S]*?)\n?```[ \t]*$/gm;
  let last = 0;
  let n = 0;
  for (let m = fence.exec(bounded); m; m = fence.exec(bounded)) {
    out.push(<Blocks key={`t${n}`} keyPrefix={`t${n}`} text={bounded.slice(last, m.index)} />);
    out.push(
      <pre key={`c${n}`} className="rich-code">
        <code>{m[1]}</code>
      </pre>,
    );
    last = fence.lastIndex;
    n++;
  }
  out.push(<Blocks key={`t${n}`} keyPrefix={`t${n}`} text={bounded.slice(last)} />);
  return <div className={className ? `rich-text ${className}` : 'rich-text'}>{out}</div>;
}

/** Former name kept for existing call sites. */
export function TutorText({ text }: { text: string }) {
  return <RichText text={text} />;
}
