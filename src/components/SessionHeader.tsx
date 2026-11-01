import type { ReactNode } from 'react';
import type { Phase } from '../lesson';
import { Icon, type IconName } from './Icon';
import { Menu, MenuItem } from './Menu';

export type PanelKind = 'plan' | 'visual' | 'sources';
type PanelToggle = {
  kind: PanelKind;
  label: string;
  icon: IconName;
  active: boolean;
  disabled: boolean;
  title: string;
  note?: string;
};

/**
 * Where the learner is in the lesson: phases behind them, the current one, and what remains. A phase behind is not a
 * claim of completion — a learner can skip ahead to a check — so it is marked as passed, never ticked.
 */
export function PhaseStepper({ phases, stage }: { phases: Phase[]; stage: string }) {
  const at = phases.findIndex((p) => p.stages.includes(stage));
  if (!phases.length || at < 0) return null;
  return (
    <ol
      className="stepper"
      aria-label={`Lesson progress: ${phases[at].label}, step ${at + 1} of ${phases.length}`}
    >
      {phases.map((p, i) => (
        <li
          key={p.id}
          data-state={i < at ? 'done' : i === at ? 'current' : 'todo'}
          aria-current={i === at ? 'step' : undefined}
        >
          <span className="stepper-dot" aria-hidden="true" />
          <span className="stepper-label">{p.label}</span>
        </li>
      ))}
    </ol>
  );
}

/**
 * The study header: which goal and capability this is, lesson progress, the side panels, and session controls
 * (quiet work, stop, resume) tucked into one menu.
 */
export function SessionHeader({
  crumbs,
  title,
  phases,
  stage,
  panels,
  paused,
  stopped,
  working,
  collapsed,
  extra,
  onPanel,
  onSessionAction,
  onExpand,
}: {
  crumbs: string[];
  title: string;
  phases: Phase[];
  stage: string;
  panels: PanelToggle[];
  paused: boolean;
  stopped: boolean;
  working: boolean;
  collapsed: boolean;
  extra?: ReactNode;
  onPanel: (kind: PanelKind) => void;
  onSessionAction: (action: 'quiet' | 'stop' | 'resume') => void;
  onExpand: () => void;
}) {
  return (
    <header className="session-header">
      {collapsed ? (
        <button
          className="icon-button"
          onClick={onExpand}
          aria-label="Show sidebar"
          title="Show sidebar (⌘\)"
        >
          <Icon name="sidebar" />
        </button>
      ) : null}
      <div className="session-title">
        {crumbs.length ? (
          <p className="crumbs">
            {crumbs.map((c, i) => (
              <span key={i}>{c}</span>
            ))}
          </p>
        ) : null}
        <h1 title={title}>{title}</h1>
      </div>
      <PhaseStepper phases={phases} stage={stage} />
      <div className="header-actions">
        {extra}
        <div className="panel-toggles" role="group" aria-label="Side panel">
          {panels.map((p) => (
            <button
              key={p.kind}
              className="toggle"
              aria-label={p.label}
              aria-pressed={p.active}
              disabled={p.disabled}
              title={p.title}
              onClick={() => onPanel(p.kind)}
            >
              <Icon name={p.icon} />
              <span className="toggle-label">{p.label}</span>
              {p.note ? <span className="toggle-note">{p.note}</span> : null}
            </button>
          ))}
        </div>
        <Menu
          className="session-menu"
          icon="more"
          label={<span className="sr-only">Session options</span>}
          title="Session options"
          disabled={working}
          placement="bottom"
          align="end"
          chevron={false}
          panelClassName="session-menu-panel"
        >
          {(close) =>
            paused ? (
              <MenuItem
                icon="play"
                label={stopped ? 'Resume where you stopped' : 'End quiet work'}
                onSelect={() => {
                  close();
                  onSessionAction('resume');
                }}
              />
            ) : (
              <>
                <MenuItem
                  icon="moon"
                  label="Quiet work"
                  detail="No tutor requests or prompts until you resume"
                  onSelect={() => {
                    close();
                    onSessionAction('quiet');
                  }}
                />
                <MenuItem
                  icon="pause"
                  label="Stop for now"
                  detail="Your place and work are saved"
                  onSelect={() => {
                    close();
                    onSessionAction('stop');
                  }}
                />
              </>
            )
          }
        </Menu>
      </div>
    </header>
  );
}
