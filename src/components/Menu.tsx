import { useEffect, useId, useRef, useState } from 'react';
import type { KeyboardEvent, ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

/**
 * A small popover menu: a trigger button and a panel of menu items. Arrow keys move between items, Escape or a click
 * outside closes it and returns focus to the trigger. `children` receives `close` so an item can dismiss the menu.
 */
export function Menu({
  label,
  icon,
  title,
  disabled = false,
  placement = 'top',
  align = 'start',
  className,
  panelClassName,
  children,
}: {
  /** Visible trigger content. */
  label: ReactNode;
  icon?: IconName;
  /** Tooltip and accessible description for the trigger. */
  title?: string;
  disabled?: boolean;
  placement?: 'top' | 'bottom';
  align?: 'start' | 'end';
  className?: string;
  panelClassName?: string;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  const close = (refocus = true) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };
  useEffect(() => {
    if (!open) return;
    const items = panel.current?.querySelectorAll<HTMLElement>('[role^="menuitem"]:not(:disabled)');
    const checked = panel.current?.querySelector<HTMLElement>('[aria-checked="true"]');
    (checked ?? items?.[0])?.focus();
    const outside = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open]);
  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
      return;
    }
    if (e.key === 'Tab') {
      setOpen(false);
      return;
    }
    const items = [
      ...(panel.current?.querySelectorAll<HTMLElement>('[role^="menuitem"]:not(:disabled)') ?? []),
    ];
    if (!items.length) return;
    const at = items.indexOf(document.activeElement as HTMLElement);
    const next =
      e.key === 'ArrowDown'
        ? items[(at + 1) % items.length]
        : e.key === 'ArrowUp'
          ? items[(at - 1 + items.length) % items.length]
          : e.key === 'Home'
            ? items[0]
            : e.key === 'End'
              ? items[items.length - 1]
              : null;
    if (next) {
      e.preventDefault();
      next.focus();
    }
  };
  return (
    <div
      className={className ? `menu ${className}` : 'menu'}
      ref={root}
      data-open={open || undefined}
    >
      <button
        ref={trigger}
        type="button"
        className="menu-trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        title={title}
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(e) => {
          if (!open && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        {icon ? <Icon name={icon} /> : null}
        {label}
        <Icon name="chevronDown" />
      </button>
      {open ? (
        <div
          ref={panel}
          id={id}
          role="menu"
          className={panelClassName ? `menu-panel ${panelClassName}` : 'menu-panel'}
          data-placement={placement}
          data-align={align}
          onKeyDown={onKeyDown}
        >
          {children(() => close())}
        </div>
      ) : null}
    </div>
  );
}

/** One entry in a Menu: an action, or a choice when `checked` is given. */
export function MenuItem({
  icon,
  label,
  detail,
  aside,
  checked,
  disabled = false,
  title,
  onSelect,
}: {
  icon?: IconName;
  label: ReactNode;
  detail?: ReactNode;
  /** Right-aligned supplementary value, such as a price. */
  aside?: ReactNode;
  checked?: boolean;
  disabled?: boolean;
  title?: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role={checked === undefined ? 'menuitem' : 'menuitemradio'}
      aria-checked={checked}
      className="menu-item"
      disabled={disabled}
      title={title}
      tabIndex={-1}
      onClick={onSelect}
    >
      {icon ? <Icon name={icon} /> : null}
      <span className="menu-item-text">
        <span>{label}</span>
        {detail ? <small>{detail}</small> : null}
      </span>
      {aside ? <span className="menu-item-aside">{aside}</span> : null}
      {checked ? (
        <Icon name="check" />
      ) : checked === false ? (
        <span className="menu-check-space" />
      ) : null}
    </button>
  );
}
