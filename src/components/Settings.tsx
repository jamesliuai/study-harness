import { useEffect, useRef, useState } from 'react';
import { command, money } from '../api';
import type {
  CredentialStatus,
  Diagnostics,
  PrivacySettings,
  Workspace,
} from '../generated/contracts';
import { DataControls } from './DataControls';
import { LocalGuide } from './LocalGuide';
import { Icon, type IconName } from './Icon';
import { EFFORTS } from './ModelControls';
import { readTheme, saveTheme, type ThemePreference } from '../theme';
const APPEARANCE: { id: ThemePreference; label: string; icon: IconName }[] = [
  { id: 'system', label: 'Match macOS', icon: 'monitor' },
  { id: 'light', label: 'Light', icon: 'sun' },
  { id: 'dark', label: 'Dark', icon: 'moon' },
];
const ERROR_KIND: Record<string, string> = {
  authentication: 'The key was rejected',
  model_access: 'This key’s project cannot use a required model',
  billing: 'No API credit available',
  rate_limit: 'Rate limited',
  network: 'Network failure',
  provider_error: 'OpenAI service problem',
  budget: 'Stopped by your local spending limit',
  no_key: 'No key available',
};
const TABS: { id: string; label: string; icon: IconName }[] = [
  { id: 'tutor', label: 'Tutor', icon: 'chip' },
  { id: 'spending', label: 'Spending', icon: 'gauge' },
  { id: 'study', label: 'Study & display', icon: 'book' },
  { id: 'data', label: 'Your data', icon: 'database' },
  { id: 'advanced', label: 'Advanced', icon: 'wrench' },
];
const usd = (n: number) => `$${Number.isInteger(n) ? n : n.toFixed(2)}`;
function Meter({ spent, limit, label }: { spent: number; limit: number; label: string }) {
  const share = limit ? Math.min(1, spent / limit) : 1;
  return (
    <div className="limit-meter" data-near={share >= 0.8 || undefined}>
      <div className="limit-meter-head">
        <span>{label}</span>
        <span className="numeric">
          {money(spent)} <span className="muted">of {money(limit)}</span>
        </span>
      </div>
      <span className="meter" aria-hidden="true">
        <span style={{ width: `${share * 100}%` }} />
      </span>
    </div>
  );
}
export function Settings({
  workspace,
  close,
  refresh,
  error,
  openComparison,
}: {
  workspace: Workspace;
  close: () => void;
  refresh: () => Promise<void>;
  error: (text: string) => void;
  openComparison: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [tab, setTab] = useState('tutor');
  const [key, setKey] = useState(''),
    [persist, setPersist] = useState(!workspace.credential.storage.includes('Keychain disabled')),
    [busy, setBusy] = useState(false);
  const [status, setStatus] = useState(workspace.credential),
    [sessionLimit, setSessionLimit] = useState(workspace.budget.limits.session_microusd / 1e6),
    [monthLimit, setMonthLimit] = useState(workspace.budget.limits.monthly_microusd / 1e6),
    [notice, setNotice] = useState('');
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [theme, setTheme] = useState(readTheme);
  const provider = workspace.provider;
  const privacy = workspace.privacy;
  const models = workspace.models ?? [];
  useEffect(() => {
    const d = dialog.current;
    // Focus lands on the dialog itself, not on the close button.
    d?.setAttribute('autofocus', '');
    d?.showModal();
    return () => d?.close();
  }, []);
  const act = async (f: () => Promise<void>) => {
    setBusy(true);
    setNotice('');
    try {
      await f();
      await refresh();
    } catch (e) {
      error(String(e));
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  };
  const prefer = (patch: Partial<PrivacySettings>, saved?: string) =>
    void act(async () => {
      await command('update_privacy', { privacy: { ...privacy, ...patch } });
      if (saved) setNotice(saved);
    });
  const select = (id: string) => {
    setTab(id);
    setNotice('');
  };
  return (
    <dialog
      ref={dialog}
      className="settings-dialog"
      tabIndex={-1}
      onCancel={close}
      aria-labelledby="settings-title"
    >
      <header className="dialog-header">
        <h2 id="settings-title">Settings</h2>
        <button className="dialog-close" onClick={close} aria-label="Close settings">
          <Icon name="close" />
        </button>
      </header>
      <div className="settings-body">
        <div
          className="settings-nav"
          role="tablist"
          aria-orientation="vertical"
          aria-label="Settings sections"
          onKeyDown={(e) => {
            if (!['ArrowDown', 'ArrowUp'].includes(e.key)) return;
            e.preventDefault();
            const at = TABS.findIndex((t) => t.id === tab);
            const next = TABS[(at + (e.key === 'ArrowDown' ? 1 : -1) + TABS.length) % TABS.length];
            select(next.id);
            document.getElementById(`settings-tab-${next.id}`)?.focus();
          }}
        >
          {TABS.map((t) => (
            <button
              key={t.id}
              id={`settings-tab-${t.id}`}
              role="tab"
              aria-selected={tab === t.id}
              aria-controls={`settings-panel-${t.id}`}
              tabIndex={tab === t.id ? 0 : -1}
              onClick={() => select(t.id)}
            >
              <Icon name={t.icon} />
              {t.label}
              {t.id === 'tutor' &&
              !workspace.offline &&
              ['not_configured', 'attention'].includes(provider.state) ? (
                <span className="setup-dot" data-state={provider.state} aria-hidden="true" />
              ) : null}
            </button>
          ))}
        </div>
        <div
          className="settings-panel"
          role="tabpanel"
          id={`settings-panel-${tab}`}
          aria-labelledby={`settings-tab-${tab}`}
        >
          {tab === 'tutor' ? (
            <>
              {workspace.offline ? (
                <section>
                  <h3>Offline development</h3>
                  <p>
                    Credentials, live transcription, outbound provider requests and billing are
                    disabled. Scenarios use a separate development database.
                  </p>
                </section>
              ) : (
                <section>
                  <h3>OpenAI key</h3>
                  <div className="provider-state" data-state={provider.state}>
                    <p className="provider-state-head">
                      <Icon
                        name={
                          provider.state === 'verified'
                            ? 'checkCircle'
                            : provider.state === 'attention'
                              ? 'alert'
                              : 'key'
                        }
                      />
                      <strong>
                        {provider.state === 'not_configured'
                          ? 'Not set up'
                          : provider.state === 'saved_unverified'
                            ? 'Key saved — not yet verified'
                            : provider.state === 'verified'
                              ? 'Connected'
                              : 'Needs attention'}
                      </strong>
                      {provider.state === 'verified' && provider.verified_at ? (
                        <span className="muted">
                          · confirmed {new Date(provider.verified_at).toLocaleString()}
                        </span>
                      ) : null}
                    </p>
                    <p className="small">
                      {provider.state === 'not_configured'
                        ? 'One OpenAI API key covers the tutor and optional recorded-speech transcription.'
                        : provider.state === 'saved_unverified'
                          ? 'Saving a key sends nothing to OpenAI. Your first tutor reply confirms access; you can also check access below.'
                          : provider.state === 'verified'
                            ? 'A response from OpenAI reached the app with this key. Live quality is still for you to judge.'
                            : `${ERROR_KIND[provider.last_error_kind ?? ''] ?? 'Last request failed'}: ${provider.last_error ?? ''}`}
                    </p>
                    <p className="small muted">
                      {status.masked} · {status.storage}. {status.message}
                    </p>
                  </div>
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      const submitted = key;
                      setKey('');
                      void act(async () => {
                        setStatus(
                          await command<CredentialStatus>('set_credential', {
                            key: submitted,
                            persist,
                          }),
                        );
                        setNotice(
                          'Key saved. Nothing was sent to OpenAI; your first tutor reply will confirm access.',
                        );
                      });
                    }}
                  >
                    <label htmlFor="api-key">API key</label>
                    <div className="search-row">
                      <input
                        autoComplete="off"
                        spellCheck={false}
                        id="api-key"
                        type="password"
                        value={key}
                        onChange={(e) => setKey(e.target.value)}
                        placeholder="sk-…"
                      />
                      <button className="primary" disabled={!key || busy} type="submit">
                        Save key
                      </button>
                    </div>
                    <label className="check">
                      <input
                        type="checkbox"
                        checked={persist}
                        onChange={(e) => setPersist(e.target.checked)}
                      />
                      Save in macOS Keychain
                    </label>
                    <p className="small muted">
                      Uncheck to keep the key in memory only until you quit. A session key
                      temporarily overrides a saved one; Remove key deletes the saved credential
                      too.
                    </p>
                    <div className="actions">
                      <button
                        type="button"
                        disabled={busy || !status.configured}
                        title="Two model-lookup requests with this key; no learning content, no tokens"
                        onClick={() =>
                          void act(async () => {
                            await command('check_connection', {
                              sessionId: workspace.active_session_id,
                            });
                            setNotice(
                              'OpenAI accepted the key and both models are available to it.',
                            );
                          })
                        }
                      >
                        Check access
                      </button>
                      <button
                        type="button"
                        className="ghost"
                        disabled={busy || !status.configured}
                        onClick={() =>
                          void act(async () => {
                            setStatus(await command<CredentialStatus>('remove_credential'));
                            setNotice('Credential removed.');
                          })
                        }
                      >
                        Remove key
                      </button>
                    </div>
                    <p className="small muted">
                      Check access sends two model lookups to OpenAI with the key — no learning
                      content and no generated tokens. It never runs automatically.
                    </p>
                  </form>
                  <details className="billing-help disclosure">
                    <summary>Billing and what is sent</summary>
                    <p className="small">
                      Study Harness uses an OpenAI API key and your API organization’s credit
                      balance, not a ChatGPT sign-in. Create a project key at platform.openai.com
                      and make sure that organization has API credit. Eligible ChatGPT Pro or
                      Business subscribers can link an API organization they own and claim monthly
                      API credits in billing settings; eligibility, amounts and model access depend
                      on your accounts. When no credit remains, requests fail and your saved work is
                      unaffected.
                    </p>
                    <p className="small">
                      What goes to OpenAI: when you make or change a plan, your goal text and a few
                      passages from material you attached; when a new task is written, the plan, the
                      capability and (for teaching steps) your recent answers; when you send to the
                      tutor, your submission, the current task and a bounded recent conversation.
                      Audio goes only when you choose Transcribe. Everything is stored on this Mac.
                    </p>
                  </details>
                </section>
              )}
              {models.length ? (
                <section>
                  <h3>Model</h3>
                  <p className="small">
                    Used for plans, new tasks and tutor replies from now on; a request already sent
                    keeps its model. You can also switch from the composer.
                  </p>
                  <div className="model-options" role="radiogroup" aria-label="Tutor model">
                    {models.map((m) => (
                      <label key={m.id} className="model-option">
                        <input
                          type="radio"
                          name="tutor-model"
                          checked={privacy.tutor_model === m.id}
                          onChange={() => prefer({ tutor_model: m.id }, `${m.label} selected.`)}
                        />
                        <span className="model-option-text">
                          <span>
                            <strong>{m.label}</strong> <code>{m.id}</code>
                          </span>
                          <small>{m.note}</small>
                        </span>
                        <span className="model-option-price numeric">
                          {usd(m.input_usd_per_million)} / {usd(m.output_usd_per_million)}
                          <small>in / out per 1M tokens</small>
                        </span>
                      </label>
                    ))}
                  </div>
                  <p className="field-label">Reasoning effort</p>
                  <div className="segmented" role="radiogroup" aria-label="Reasoning effort">
                    {EFFORTS.map((e) => (
                      <button
                        key={e.id}
                        type="button"
                        role="radio"
                        aria-checked={privacy.reasoning_effort === e.id}
                        data-active={privacy.reasoning_effort === e.id || undefined}
                        title={e.detail}
                        onClick={() =>
                          prefer({ reasoning_effort: e.id }, `${e.label} effort selected.`)
                        }
                      >
                        {e.label}
                      </button>
                    ))}
                  </div>
                  <p className="small muted">
                    Higher effort reasons longer before replying: slower, and each request reserves
                    more of your budget. Plans and new tasks always use Medium or above so their
                    answer keys stay reliable.
                  </p>
                </section>
              ) : null}
            </>
          ) : tab === 'spending' ? (
            <>
              <section>
                <h3>So far</h3>
                <Meter
                  label="This session"
                  spent={workspace.budget.session_spent_microusd}
                  limit={workspace.budget.limits.session_microusd}
                />
                <Meter
                  label={`This month (UTC, ${workspace.budget.month})`}
                  spent={workspace.budget.month_spent_microusd}
                  limit={workspace.budget.limits.monthly_microusd}
                />
                {workspace.budget.unresolved_count ? (
                  <p className="small">
                    {workspace.budget.unresolved_count} pending or uncertain charge
                    {workspace.budget.unresolved_count === 1 ? '' : 's'} counted conservatively.
                  </p>
                ) : null}
              </section>
              <section>
                <h3>Limits</h3>
                <p className="small">
                  New requests stop when their conservative reservation would pass a limit. Limits
                  cover this local profile only — other profiles and other uses of your key spend
                  separately.
                </p>
                <div className="limit-fields">
                  <label>
                    Per session (USD)
                    <input
                      type="number"
                      min="0"
                      max="100"
                      step=".1"
                      value={sessionLimit}
                      onChange={(e) => setSessionLimit(Number(e.target.value))}
                    />
                  </label>
                  <label>
                    Per month (USD)
                    <input
                      type="number"
                      min="0"
                      max="1000"
                      step="1"
                      value={monthLimit}
                      onChange={(e) => setMonthLimit(Number(e.target.value))}
                    />
                  </label>
                </div>
                <button
                  disabled={
                    busy ||
                    !Number.isFinite(sessionLimit) ||
                    !Number.isFinite(monthLimit) ||
                    (Math.round(sessionLimit * 1e6) === workspace.budget.limits.session_microusd &&
                      Math.round(monthLimit * 1e6) === workspace.budget.limits.monthly_microusd)
                  }
                  onClick={() =>
                    void act(async () => {
                      await command('update_limits', {
                        sessionId: workspace.active_session_id,
                        limits: {
                          session_microusd: Math.round(sessionLimit * 1e6),
                          monthly_microusd: Math.round(monthLimit * 1e6),
                        },
                      });
                      setNotice('Limits saved.');
                    })
                  }
                >
                  Save limits
                </button>
              </section>
              <section>
                <h3>Recent requests</h3>
                {workspace.usage.length ? (
                  <ul className="usage-list">
                    {workspace.usage.map((u) => (
                      <li key={u.request_id}>
                        <code>{u.model}</code>
                        <span className="numeric">{money(u.cost_microusd)}</span>
                        <span className="muted">
                          {u.cost_status} · {u.status}
                        </span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="small muted">No completed provider requests yet.</p>
                )}
              </section>
            </>
          ) : tab === 'study' ? (
            <>
              <section>
                <h3>Appearance</h3>
                <div
                  className="segmented appearance-options"
                  role="radiogroup"
                  aria-label="Appearance"
                >
                  {APPEARANCE.map((a) => (
                    <button
                      key={a.id}
                      type="button"
                      role="radio"
                      aria-checked={theme === a.id}
                      onClick={() => {
                        setTheme(a.id);
                        saveTheme(a.id);
                      }}
                    >
                      <Icon name={a.icon} />
                      {a.label}
                    </button>
                  ))}
                </div>
                <p className="small muted">
                  Kept on this Mac only. An open interactive visual switches the next time it loads.
                </p>
              </section>
              <section>
                <h3>Text size</h3>
                <div className="segmented" role="radiogroup" aria-label="Text size">
                  {[100, 125, 150, 175, 200].map((n) => (
                    <button
                      key={n}
                      type="button"
                      role="radio"
                      aria-checked={privacy.text_scale_percent === n}
                      data-active={privacy.text_scale_percent === n || undefined}
                      onClick={() => prefer({ text_scale_percent: n })}
                    >
                      {n}%
                    </button>
                  ))}
                </div>
              </section>
              <section>
                <h3>Reviews</h3>
                <label>
                  Planned review minutes per day
                  <input
                    type="number"
                    min="3"
                    max="60"
                    defaultValue={privacy.review_minutes}
                    disabled={busy}
                    onBlur={(e) => {
                      const n = Number(e.target.value);
                      if (n >= 3 && n <= 60 && n !== privacy.review_minutes)
                        prefer({ review_minutes: n }, 'Review time saved.');
                      else e.target.value = String(privacy.review_minutes);
                    }}
                  />
                </label>
                <p className="small muted">
                  Review offers are capped at five items and roughly three minutes each. You can
                  stop sooner; overdue means ready to revisit, not failed.
                </p>
              </section>
              <section>
                <h3>Recorded speech</h3>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={privacy.retain_failed_audio}
                    disabled={busy}
                    onChange={(e) =>
                      prefer(
                        { retain_failed_audio: e.target.checked },
                        'Recording retention preference saved.',
                      )
                    }
                  />
                  Keep audio in memory for a retry after a failed transcription
                </label>
                <p className="small muted">
                  Audio is never saved to disk. It is released after a successful transcription,
                  discard, activity change or exit; turn this off to discard it after any
                  transcription request. Confirmed text stays in your record until you delete its
                  session.
                </p>
              </section>
            </>
          ) : tab === 'data' ? (
            <DataControls workspace={workspace} refresh={refresh} />
          ) : (
            <>
              <section>
                <h3>Research tools</h3>
                <p className="small">
                  Personal comparison records keep held-out assessments outside tutoring. They are
                  optional and separate from ordinary study.
                </p>
                <button onClick={openComparison}>
                  <Icon name="compare" />
                  Open learning comparison
                </button>
              </section>
              <section>
                <h3>Technical details</h3>
                <p className="small">
                  Tutor: <code>{workspace.provider_model}</code> · speech:{' '}
                  <code>{workspace.transcription_model}</code> · rates:{' '}
                  <code>{workspace.rate_version}</code>
                </p>
                <details>
                  <summary>Request record for this session ({workspace.requests.length})</summary>
                  <ul className="small">
                    {workspace.requests.slice(0, 12).map((r) => (
                      <li key={r.id}>
                        {new Date(r.created_at).toLocaleString()} · {r.kind} · {r.status} ·{' '}
                        {workspace.usage.find((u) => u.request_id === r.id)?.cost_status ??
                          'pending reservation'}
                      </li>
                    ))}
                  </ul>
                </details>
                <details>
                  <summary>Frozen request versions</summary>
                  {workspace.request_versions.map((v) => (
                    <article key={v.request_id} className="small">
                      <p>
                        {v.prompt_version} · requested {v.requested_model}
                        {v.reasoning_effort ? ` (${v.reasoning_effort} effort)` : ''}
                        {v.reported_model
                          ? ` · provider reported ${v.reported_model}`
                          : ' · provider did not report a model identifier'}
                      </p>
                      <p>
                        {v.content_version} · {v.scheduler_version} · {v.policy_version} ·{' '}
                        {v.provenance}
                      </p>
                      <details>
                        <summary>Exact version hashes</summary>
                        <pre>{JSON.stringify(v, null, 2)}</pre>
                      </details>
                    </article>
                  ))}
                </details>
                <div className="actions">
                  <button
                    className="ghost"
                    onClick={() =>
                      void act(async () =>
                        setDiagnostics(await command<Diagnostics>('diagnostics')),
                      )
                    }
                  >
                    Show local diagnostics
                  </button>
                </div>
                {diagnostics ? <pre>{JSON.stringify(diagnostics, null, 2)}</pre> : null}
              </section>
              <section>
                <LocalGuide />
              </section>
            </>
          )}
          <p className="settings-notice" role="status">
            {notice}
          </p>
        </div>
      </div>
    </dialog>
  );
}
