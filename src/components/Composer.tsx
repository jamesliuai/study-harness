import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { command, money } from '../api';
import { preferredMime, recordingToWav } from '../audio';
import type { RecordingTicket, Transcript } from '../generated/contracts';
import { Icon } from './Icon';
export type Intent = 'answer' | 'question';
const MAX_SECONDS = 120;
// The core measures answers in bytes (UTF-8), not characters.
const MAX_BYTES = 12000;
const encoder = new TextEncoder();
const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
export function Composer({
  initialDraft,
  initialTranscript,
  sessionId,
  revision,
  configured,
  offline = false,
  retainFailedAudio = true,
  intents,
  defaultIntent,
  onIntent,
  tutorReady,
  answerLabel = 'Check answer',
  placeholder,
  tools,
  tray,
  help,
  context,
  onSend,
  onDraft,
  refresh,
  flushDraft,
  onWorking,
  error,
  disabled,
}: {
  initialDraft: string;
  initialTranscript: Transcript | null;
  sessionId: string;
  revision: number;
  /** A live key is available for recorded-speech transcription. */
  configured: boolean;
  offline?: boolean;
  retainFailedAudio?: boolean;
  /** What this task accepts: answers and/or questions. Questions are discussion and are never graded. */
  intents: Intent[];
  defaultIntent: Intent;
  /** Reports the learner's Answer/Question choice so it survives a remount. */
  onIntent?: (intent: Intent) => void;
  /** A tutor response can be requested (live key saved, or offline simulated responses). */
  tutorReady: boolean;
  answerLabel?: string;
  /** Placeholder for an answer; questions use their own. */
  placeholder?: string;
  /** Extra controls at the end of the bar (the model and effort pickers). */
  tools?: ReactNode;
  /** The status line attached under the composer (spending, setup state). */
  tray?: ReactNode;
  /** The help menu (hints, explanations, the solution), shown in the composer bar. */
  help?: ReactNode;
  /** A line above the box saying which task this answers. */
  context?: ReactNode;
  /** Saves first (with frozen conditions); `tutor` then requests a response for the saved submission. */
  onSend: (
    text: string,
    transcriptId: string | null,
    id: string,
    revision: number,
    intent: Intent,
    tutor: boolean,
  ) => Promise<void>;
  onDraft: (text: string) => void;
  refresh: () => Promise<void>;
  flushDraft: () => Promise<void>;
  onWorking: (working: boolean) => void;
  error: (s: string) => void;
  disabled: boolean;
}) {
  const [text, setText] = useState(initialDraft),
    [recording, setRecording] = useState(false),
    [working, setWorking] = useState(false),
    [phase, setPhase] = useState(''),
    [ticket, setTicket] = useState<RecordingTicket | null>(null),
    [wav, setWav] = useState<Uint8Array | null>(null),
    [transcript, setTranscript] = useState<Transcript | null>(initialTranscript),
    [seconds, setSeconds] = useState(0),
    [intent, setIntent] = useState<Intent>(defaultIntent);
  const recorder = useRef<MediaRecorder | null>(null),
    stream = useRef<MediaStream | null>(null),
    timer = useRef<ReturnType<typeof setTimeout> | null>(null),
    started = useRef(0),
    mounted = useRef(true),
    recordingId = useRef<string | null>(null),
    box = useRef<HTMLTextAreaElement>(null),
    submission = useRef(crypto.randomUUID());
  const busy = (label: string) => {
    setWorking(true);
    setPhase(label);
  };
  const idle = () => {
    setWorking(false);
    setPhase('');
  };
  const stop = () => {
    if (recorder.current?.state === 'recording') recorder.current.stop();
    stream.current?.getTracks().forEach((t) => t.stop());
    setRecording(false);
    if (timer.current) clearTimeout(timer.current);
  };
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (timer.current) clearTimeout(timer.current);
      if (recorder.current) recorder.current.onstop = null;
      if (recorder.current?.state === 'recording') recorder.current.stop();
      stream.current?.getTracks().forEach((t) => t.stop());
      if (recordingId.current)
        void command('discard_recording', { recordingId: recordingId.current }).catch(() => {});
    };
  }, []);
  useEffect(() => {
    onWorking(working);
    return () => onWorking(false);
  }, [working, onWorking]);
  useEffect(() => {
    if (!recording) return;
    const id = setInterval(
      () => setSeconds(Math.floor((performance.now() - started.current) / 1000)),
      250,
    );
    return () => clearInterval(id);
  }, [recording]);
  // Grow with the text up to the stylesheet's max-height, then scroll.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight + 2}px`;
  }, [text, transcript]);
  const start = async () => {
    busy('Starting the microphone…');
    try {
      const mime = preferredMime();
      if (!mime || !navigator.mediaDevices?.getUserMedia)
        throw new Error(
          'This WebView cannot record a supported format. The text path remains available.',
        );
      const nextTicket = await command<RecordingTicket>('begin_recording', { sessionId });
      recordingId.current = nextTicket.id;
      if (!mounted.current) {
        await command('discard_recording', { recordingId: nextTicket.id });
        return;
      }
      setTicket(nextTicket);
      const media = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
      if (!mounted.current) {
        media.getTracks().forEach((track) => track.stop());
        await command('discard_recording', { recordingId: nextTicket.id });
        return;
      }
      stream.current = media;
      const rec = new MediaRecorder(stream.current, { mimeType: mime });
      recorder.current = rec;
      const parts: BlobPart[] = [];
      rec.ondataavailable = (e) => {
        if (e.data.size) parts.push(e.data);
      };
      rec.onerror = () => {
        error('Microphone recording failed. Try again or type your reasoning.');
        stop();
      };
      rec.onstop = async () => {
        if (!mounted.current) return;
        busy('Preparing the recording…');
        try {
          await command('end_recording', { recordingId: nextTicket.id });
          const data = await recordingToWav(new Blob(parts, { type: rec.mimeType }));
          if (mounted.current) setWav(data);
        } catch (e) {
          if (mounted.current) error(String(e));
          await command('discard_recording', { recordingId: nextTicket.id });
          recordingId.current = null;
          if (mounted.current) setTicket(null);
        } finally {
          if (mounted.current) idle();
        }
      };
      rec.start();
      started.current = performance.now();
      setSeconds(0);
      setRecording(true);
      timer.current = setTimeout(stop, (MAX_SECONDS - 1) * 1000);
    } catch (e) {
      stop();
      if (recordingId.current)
        await command('discard_recording', { recordingId: recordingId.current }).catch(() => {});
      recordingId.current = null;
      if (mounted.current) {
        setTicket(null);
        error(
          e instanceof DOMException && e.name === 'NotAllowedError'
            ? 'Microphone access was denied. You can type and save, or enable Study Harness in System Settings → Privacy & Security → Microphone and try again.'
            : String(e),
        );
      }
    } finally {
      if (mounted.current) idle();
    }
  };
  const discard = async () => {
    if (recordingId.current)
      await command('discard_recording', { recordingId: recordingId.current });
    recordingId.current = null;
    setWav(null);
    setTicket(null);
  };
  const transcribe = async () => {
    if (!wav || !ticket) return;
    busy('Transcribing…');
    try {
      await flushDraft();
      const t = await command<Transcript>('transcribe', {
        sessionId,
        recordingId: ticket.id,
        wav: Array.from(wav),
      });
      if (mounted.current) {
        // Speech adds to what was already typed instead of replacing it.
        const combined = text.trim()
          ? `${text.trimEnd()}\n${t.recognized_text}`
          : t.recognized_text;
        setTranscript(t);
        setText(combined);
        if (combined !== t.recognized_text) {
          onDraft(combined);
          await flushDraft();
        }
        submission.current = crypto.randomUUID();
        setWav(null);
        setTicket(null);
        recordingId.current = null;
      }
    } catch (e) {
      if (mounted.current) error(String(e));
      if (!retainFailedAudio) await discard().catch(() => {});
    } finally {
      if (mounted.current) idle();
      await refresh();
    }
  };
  const kind: Intent = intents.includes(intent) ? intent : (intents[0] ?? 'question');
  const send = async (tutor: boolean) => {
    busy(tutor ? 'Saving, then asking the tutor…' : 'Saving…');
    try {
      await onSend(text, transcript?.id ?? null, submission.current, revision, kind, tutor);
      setText('');
      setTranscript(null);
      submission.current = crypto.randomUUID();
    } catch (e) {
      error(String(e));
    } finally {
      idle();
    }
  };
  const bytes = encoder.encode(text).length;
  const tooLong = bytes > MAX_BYTES;
  const ready = !!text.trim() && !tooLong && !disabled && !working && !recording;
  const primary = tutorReady
    ? kind === 'answer'
      ? transcript
        ? `Confirm and ${answerLabel.toLowerCase()}`
        : answerLabel
      : transcript
        ? 'Confirm and ask'
        : 'Ask tutor'
    : transcript
      ? 'Confirm and save'
      : kind === 'answer'
        ? 'Save answer'
        : 'Save question';
  const recorded = wav ? (wav.length - 44) / 32000 : 0;
  const choose = (option: Intent) => {
    setIntent(option);
    onIntent?.(option);
    submission.current = crypto.randomUUID();
  };
  return (
    <section className="composer" aria-label="Response composer">
      {context}
      <div
        className="composer-box"
        data-state={recording ? 'recording' : transcript ? 'transcript' : undefined}
        data-disabled={disabled || undefined}
      >
        {transcript ? (
          <div className="composer-banner">
            <Icon name="mic" />
            <div>
              <p>
                <strong>Check the transcript, then confirm.</strong> Fix anything misheard. Nothing
                is graded or submitted until you confirm; the draft and the original text stay on
                this Mac until then.
              </p>
              <details>
                <summary>Original recognized text</summary>
                <p>{transcript.recognized_text}</p>
              </details>
            </div>
          </div>
        ) : null}
        <label htmlFor="reasoning" className="sr-only">
          {transcript
            ? 'Confirm your transcript'
            : kind === 'answer'
              ? 'Your answer'
              : 'Your question'}
        </label>
        <textarea
          id="reasoning"
          ref={box}
          rows={2}
          value={text}
          placeholder={
            kind === 'answer'
              ? (placeholder ?? 'Show your reasoning — wrap maths in $…$ to typeset it')
              : 'Ask about any step, e.g. “why does that step follow?”'
          }
          maxLength={12000}
          onChange={(e) => {
            setText(e.target.value);
            submission.current = crypto.randomUUID();
            // A draft the core would refuse is kept on screen, not written over the last saved one.
            if (encoder.encode(e.target.value).length <= MAX_BYTES) onDraft(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && ready) {
              e.preventDefault();
              void send(tutorReady);
            }
          }}
          disabled={disabled || working}
        />
        {recording ? (
          <div className="composer-status" data-kind="recording" role="status">
            <span className="rec-dot" aria-hidden="true" />
            <span>
              Recording <span className="numeric">{clock(seconds)}</span>
              <span className="muted"> / {clock(MAX_SECONDS)}</span>
            </span>
            <span className="spacer" />
            <button type="button" className="compact" onClick={stop}>
              <Icon name="stop" />
              Stop
            </button>
          </div>
        ) : wav ? (
          <div className="composer-status" data-kind="recorded">
            <Icon name="mic" />
            <span>
              <span className="numeric">{recorded.toFixed(1)} s</span> recorded ·{' '}
              {offline
                ? 'offline, nothing is uploaded'
                : `about ${money(recorded * 75)} to transcribe`}
            </span>
            <span className="spacer" />
            <button
              type="button"
              className="primary compact"
              disabled={!configured || working}
              onClick={() => void transcribe()}
            >
              Transcribe
            </button>
            <button
              type="button"
              className="ghost compact"
              disabled={working}
              onClick={() => void discard()}
            >
              Discard
            </button>
            {!configured ? (
              <p className="composer-hint">
                {offline
                  ? 'Live transcription is off offline. Use the simulated transcript control, or type.'
                  : 'Add your OpenAI key in Settings to transcribe. You can still type and save.'}
              </p>
            ) : null}
          </div>
        ) : null}
        {tooLong ? (
          <p className="composer-hint composer-limit" role="status">
            This is {(bytes - MAX_BYTES).toLocaleString()} bytes over the limit for one submission —
            shorten it or send it in parts.
          </p>
        ) : null}
        {working && phase ? (
          <p className="composer-status" data-kind="working" role="status">
            <span className="spinner" aria-hidden="true" />
            {phase}
            {phase === 'Transcribing…' ? (
              <button
                type="button"
                className="text-button"
                onClick={() => void command('cancel_request')}
              >
                Cancel
              </button>
            ) : null}
          </p>
        ) : null}
        <div className="composer-bar">
          {intents.length > 1 ? (
            <div
              className="segmented intent-switch"
              role="radiogroup"
              aria-label="What are you sending?"
              onKeyDown={(e) => {
                if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) return;
                e.preventDefault();
                const at = intents.indexOf(kind);
                const step = e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 1;
                const next = intents[(at + step + intents.length) % intents.length];
                choose(next);
                (
                  e.currentTarget.querySelector(`[data-intent="${next}"]`) as HTMLElement | null
                )?.focus();
              }}
            >
              {intents.map((option) => (
                <button
                  key={option}
                  type="button"
                  role="radio"
                  data-intent={option}
                  aria-checked={kind === option}
                  tabIndex={kind === option ? 0 : -1}
                  disabled={disabled || working}
                  title={
                    option === 'answer'
                      ? 'An attempt at this task; it can be checked'
                      : 'A question about the material; never graded'
                  }
                  onClick={() => choose(option)}
                >
                  {option === 'answer' ? 'Answer' : 'Question'}
                </button>
              ))}
            </div>
          ) : (
            <span
              className="intent-chip"
              title={kind === 'question' ? 'A question is never graded' : undefined}
            >
              <Icon name={kind === 'answer' ? 'pencil' : 'question'} />
              {kind === 'answer' ? 'Answer' : 'Question'}
            </span>
          )}
          {transcript ? (
            <button
              type="button"
              className="ghost compact"
              disabled={disabled || working}
              onClick={async () => {
                busy('Discarding…');
                try {
                  await flushDraft();
                  await command('discard_transcript', { sessionId, transcriptId: transcript.id });
                  setText('');
                  setTranscript(null);
                  submission.current = crypto.randomUUID();
                  await refresh();
                } catch (e) {
                  error(String(e));
                } finally {
                  idle();
                }
              }}
            >
              <Icon name="trash" />
              <span>Discard</span>
            </button>
          ) : (
            <button
              type="button"
              className="icon-button record-button"
              disabled={disabled || working || recording || !!wav}
              aria-label="Record your reasoning"
              title="Record up to two minutes. You choose whether to transcribe it, then check the text."
              onClick={() => void start()}
            >
              <Icon name="mic" />
            </button>
          )}
          {help}
          <span className="spacer" />
          {tools}
          <button
            type="button"
            className="send"
            aria-label={primary}
            disabled={!ready}
            title={
              tutorReady
                ? kind === 'answer'
                  ? 'Saves first, then checks it against this task (⌘↩)'
                  : 'Saves first; a question is never graded (⌘↩)'
                : 'Saves on this Mac (⌘↩)'
            }
            onClick={() => void send(tutorReady)}
          >
            <span>{primary}</span>
            <Icon name="arrowUp" />
          </button>
        </div>
      </div>
      <div className="composer-foot">
        {tray}
        <span className="spacer" />
        {tutorReady ? (
          <button
            type="button"
            className="text-button"
            disabled={!ready}
            title="Save it to your record without asking the tutor"
            onClick={() => void send(false)}
          >
            Save without feedback
          </button>
        ) : null}
        <span className="kbd-hint" aria-hidden="true">
          <kbd>⌘</kbd>
          <kbd>↩</kbd>
          {tutorReady ? ' send' : ' save'}
        </span>
      </div>
    </section>
  );
}
