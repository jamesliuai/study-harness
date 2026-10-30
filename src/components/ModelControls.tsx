import type { PrivacySettings, Workspace } from '../generated/contracts';
import { Menu, MenuItem } from './Menu';

// How long the model reasons before answering. The core maps these to request settings and validates the choice.
export const EFFORTS = [
  { id: 'low', label: 'Low', detail: 'Quick replies · the default' },
  { id: 'medium', label: 'Medium', detail: 'Thinks longer before replying' },
  { id: 'high', label: 'High', detail: 'Most careful · slower and uses more budget' },
] as const;

const usd = (n: number) => `$${Number.isInteger(n) ? n : n.toFixed(2)}`;

/**
 * Tutor model and reasoning-effort pickers. The choice is one local preference used for plans, new tasks and tutor
 * replies; a request already sent keeps the model it was frozen with.
 */
export function ModelControls({
  workspace: w,
  disabled = false,
  placement = 'top',
  onChange,
}: {
  workspace: Workspace;
  disabled?: boolean;
  placement?: 'top' | 'bottom';
  onChange: (privacy: PrivacySettings) => void;
}) {
  const models = w.models ?? [];
  if (!models.length) return null;
  const current =
    models.find((m) => m.id === w.privacy.tutor_model) ??
    models.find((m) => m.id === w.provider_model) ??
    models[0];
  const effort = EFFORTS.find((e) => e.id === w.privacy.reasoning_effort) ?? EFFORTS[0];
  const set = (patch: Partial<PrivacySettings>) => onChange({ ...w.privacy, ...patch });
  return (
    <div className="model-controls" role="group" aria-label="Tutor model and effort">
      <Menu
        className="pill-menu"
        icon="sparkle"
        label={current.label}
        title={`Tutor model: ${current.id}`}
        disabled={disabled}
        placement={placement}
        align="end"
        panelClassName="model-menu"
      >
        {(close) => (
          <>
            <p className="menu-heading">Tutor model</p>
            {models.map((m) => (
              <MenuItem
                key={m.id}
                label={
                  <>
                    {m.label} <code>{m.id}</code>
                  </>
                }
                detail={m.note}
                aside={`${usd(m.input_usd_per_million)} / ${usd(m.output_usd_per_million)}`}
                checked={m.id === current.id}
                onSelect={() => {
                  close();
                  if (m.id !== current.id) set({ tutor_model: m.id });
                }}
              />
            ))}
            <p className="menu-note">
              Prices are per million tokens, input / output. The choice applies to plans, new tasks
              and tutor replies from now on; your spending limits apply to every model.
            </p>
          </>
        )}
      </Menu>
      <Menu
        className="pill-menu"
        icon="gauge"
        label={effort.label}
        title="Reasoning effort"
        disabled={disabled}
        placement={placement}
        align="end"
        panelClassName="model-menu"
      >
        {(close) => (
          <>
            <p className="menu-heading">Reasoning effort</p>
            {EFFORTS.map((e) => (
              <MenuItem
                key={e.id}
                label={e.label}
                detail={e.detail}
                checked={e.id === effort.id}
                onSelect={() => {
                  close();
                  if (e.id !== effort.id) set({ reasoning_effort: e.id });
                }}
              />
            ))}
            <p className="menu-note">
              Plans and new tasks always think at Medium or above so their answer keys stay
              reliable.
            </p>
          </>
        )}
      </Menu>
    </div>
  );
}
