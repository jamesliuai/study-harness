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

function Inline({ text }: { text: string }) {
  return (
    <>
      {splitMath(text).map((p, i) =>
        p.kind === 'text' ? (
          <Fragment key={i}>{p.value}</Fragment>
        ) : (
          <MathText key={i} tex={p.value} block={p.kind === 'display'} />
        ),
      )}
    </>
  );
}

/**
 * Bounded, escaped prose with mathematics: paragraphs split on blank lines, single line breaks kept, numbered steps
 * (“1. …”) rendered as an ordered list, `$…$`/`$$…$$` typeset with KaTeX. Used for tasks, learner text and tutor
 * replies alike. Never renders HTML from its input.
 */
export function RichText({ text, className }: { text: string; className?: string }) {
  const bounded = text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}…` : text;
  const blocks = bounded.split(/\n\s*\n/).filter((b) => b.trim());
  const out: ReactNode[] = [];
  blocks.forEach((block, b) => {
    const lines = block.split('\n').filter((l) => l.trim());
    const isStep = (l: string) => /^\s*\d+[.)]\s+/.test(l);
    const steps = lines.filter(isStep);
    if (steps.length >= 2 && steps.length >= lines.length - 1) {
      // Lines keep their order: text before the first step introduces the list, and a line after a step
      // continues that step. The list starts at the first step's own number.
      const first = lines.findIndex(isStep);
      const lead = lines.slice(0, first);
      const items: string[][] = [];
      for (const l of lines.slice(first)) {
        if (isStep(l)) items.push([l.replace(/^\s*\d+[.)]\s+/, '')]);
        else items[items.length - 1].push(l.trim());
      }
      if (lead.length)
        out.push(
          <p key={`${b}-lead`}>
            <Inline text={lead.join(' ')} />
          </p>,
        );
      out.push(
        <ol
          key={`${b}-steps`}
          className="rich-steps"
          start={Number(/\d+/.exec(steps[0])?.[0] ?? 1) || undefined}
        >
          {items.map((item, i) => (
            <li key={i}>
              {item.map((l, j) => (
                <Fragment key={j}>
                  {j ? <br /> : null}
                  <Inline text={l} />
                </Fragment>
              ))}
            </li>
          ))}
        </ol>,
      );
      return;
    }
    const display = /^\s*\$\$[\s\S]*\$\$\s*$/.test(block);
    out.push(
      display ? (
        <div key={b} className="rich-display">
          <Inline text={block.trim()} />
        </div>
      ) : (
        <p key={b}>
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
  return <div className={className ? `rich-text ${className}` : 'rich-text'}>{out}</div>;
}

/** Former name kept for existing call sites. */
export function TutorText({ text }: { text: string }) {
  return <RichText text={text} />;
}
