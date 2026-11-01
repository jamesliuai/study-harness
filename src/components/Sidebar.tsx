import type { Workspace } from '../generated/contracts';
import { Icon, LogoMark } from './Icon';

type SessionRow = Workspace['sessions'][number];

// An unstarted session made by “New goal”: it is the home screen, not a session worth listing.
export const isPlaceholder = (s: { title: string; revision: number }) =>
  s.title === 'New session' && s.revision === 0;

/**
 * Navigation: home, due reviews and every goal with the capability sessions studied for it. Progress counts come from
 * the core (capabilities shown on your own), never from the interface.
 */
export function Sidebar({
  workspace: w,
  home,
  working,
  providerIssue,
  onHome,
  onReview,
  onOpen,
  onSettings,
  onCollapse,
}: {
  workspace: Workspace;
  home: boolean;
  working: boolean;
  providerIssue: string | null;
  onHome: () => void;
  onReview: () => void;
  onOpen: (sessionId: string) => void;
  onSettings: () => void;
  onCollapse: () => void;
}) {
  const grouped = new Set(w.goals.flatMap((g) => g.session_ids));
  const loose = w.sessions.filter((s) => !grouped.has(s.id) && !isPlaceholder(s));
  const due = w.unit.due_reviews.length;
  const row = (s: SessionRow, nested: boolean) => {
    const current = s.id === w.active_session_id && !home;
    return (
      <li key={s.id}>
        <button
          className="nav-item session-item"
          aria-current={current ? 'page' : undefined}
          data-nested={nested || undefined}
          disabled={working && !current}
          onClick={() => onOpen(s.id)}
          title={s.title}
        >
          <span className="nav-label">{s.title}</span>
          {nested ? null : (
            <span className="nav-meta numeric">
              {new Date(s.created_at).toLocaleDateString([], { month: 'short', day: 'numeric' })}
            </span>
          )}
        </button>
      </li>
    );
  };
  return (
    <nav className="sidebar" aria-label="Study sessions">
      <div className="sidebar-top">
        <div className="brand">
          <LogoMark />
          <span className="brand-name">Study Harness</span>
        </div>
        <button
          className="icon-button"
          onClick={onCollapse}
          aria-label="Hide sidebar"
          title="Hide sidebar (⌘\)"
        >
          <Icon name="sidebar" />
        </button>
      </div>
      <button
        className="new-goal"
        aria-current={home ? 'page' : undefined}
        disabled={working && !home}
        onClick={onHome}
        title="Start a new goal (⌘N)"
      >
        <Icon name="plus" />
        <span>New goal</span>
        <kbd aria-hidden="true">⌘N</kbd>
      </button>
      <ul className="nav-list" aria-label="Shortcuts">
        <li>
          <button
            className="nav-item"
            aria-current={home ? 'page' : undefined}
            disabled={working && !home}
            onClick={onHome}
          >
            <Icon name="home" />
            <span className="nav-label">Home</span>
          </button>
        </li>
        {due ? (
          <li>
            <button className="nav-item nav-review" disabled={working} onClick={onReview}>
              <Icon name="review" />
              <span className="nav-label">Reviews ready</span>
              <span className="count" aria-label={`${due} due`}>
                {due}
              </span>
            </button>
          </li>
        ) : null}
      </ul>
      <div className="session-list">
        {w.goals.map((g) => {
          const members = g.session_ids
            .map((id) => w.sessions.find((s) => s.id === id))
            .filter((s): s is SessionRow => !!s);
          if (!members.length) return null;
          const share = g.capabilities ? g.checked / g.capabilities : 0;
          return (
            <section className="nav-group" key={g.id} aria-label={g.title}>
              <h2 className="nav-group-title" title={g.title}>
                <span className="nav-label">{g.title}</span>
                {g.capabilities ? (
                  <span
                    className="progress-ring"
                    role="img"
                    aria-label={`${g.checked} of ${g.capabilities} shown on your own`}
                    title={`${g.checked} of ${g.capabilities} shown on your own`}
                    data-complete={g.checked === g.capabilities || undefined}
                  >
                    <svg viewBox="0 0 20 20" aria-hidden="true">
                      <circle cx="10" cy="10" r="7.5" />
                      <circle
                        cx="10"
                        cy="10"
                        r="7.5"
                        pathLength={100}
                        strokeDasharray={`${share * 100} 100`}
                      />
                    </svg>
                    <span className="numeric">
                      {g.checked}/{g.capabilities}
                    </span>
                  </span>
                ) : null}
              </h2>
              <ul>{members.map((s) => row(s, true))}</ul>
            </section>
          );
        })}
        {loose.length ? (
          <section className="nav-group" aria-label="Other sessions">
            <h2 className="nav-group-title">
              <span className="nav-label">{w.goals.length ? 'Other sessions' : 'Sessions'}</span>
            </h2>
            <ul>{loose.map((s) => row(s, false))}</ul>
          </section>
        ) : null}
      </div>
      <div className="sidebar-footer">
        <button className="nav-item settings-button" disabled={working} onClick={onSettings}>
          <Icon name="sliders" />
          <span className="nav-label">Settings</span>
          {providerIssue ? (
            <span
              className="setup-dot"
              data-state={w.provider.state}
              title={providerIssue}
              aria-label={providerIssue}
            />
          ) : null}
          <kbd aria-hidden="true">⌘,</kbd>
        </button>
      </div>
    </nav>
  );
}
