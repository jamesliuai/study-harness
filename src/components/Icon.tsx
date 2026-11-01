import type { ReactNode } from 'react';

const paths = {
  plus: <path d="M12 5v14M5 12h14" />,
  close: <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />,
  sliders: (
    <>
      <path d="M4 7h9M18 7h2M4 17h3M12 17h8" />
      <circle cx="15.5" cy="7" r="2.25" />
      <circle cx="9.5" cy="17" r="2.25" />
    </>
  ),
  compare: (
    <>
      <rect x="4" y="10" width="6" height="10" rx="1.5" />
      <rect x="14" y="4" width="6" height="16" rx="1.5" />
    </>
  ),
  mic: (
    <>
      <rect x="9" y="3.5" width="6" height="10.5" rx="3" />
      <path d="M5.75 11a6.25 6.25 0 0 0 12.5 0M12 17.25V20.5" />
    </>
  ),
  stop: <rect x="6.5" y="6.5" width="11" height="11" rx="2" />,
  arrow: <path d="M5 12h14M13.5 6.5L19 12l-5.5 5.5" />,
  arrowUp: <path d="M12 19V5.5M6.5 11L12 5.5l5.5 5.5" />,
  chevronDown: <path d="M7 10l5 5 5-5" />,
  chevronRight: <path d="M10 7l5 5-5 5" />,
  check: <path d="M5 12.5l4.25 4.25L19 7" />,
  clip: (
    <path d="M15.5 7.5l-6.25 6.25a1.75 1.75 0 0 0 2.5 2.5l6.5-6.5a3.5 3.5 0 0 0-5-5l-6.75 6.75a5.25 5.25 0 0 0 7.43 7.43L19 13.9" />
  ),
  list: <path d="M9 7h11M9 12h11M9 17h11M4.5 7h.01M4.5 12h.01M4.5 17h.01" />,
  alert: (
    <>
      <circle cx="12" cy="12" r="8.25" />
      <path d="M12 7.75v5M12 16.25v.25" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="8.25" />
      <path d="M12 11v5M12 7.75v.25" />
    </>
  ),
  chip: (
    <>
      <rect x="7" y="7" width="10" height="10" rx="2" />
      <path d="M10 3.75V7M14 3.75V7M10 17v3.25M14 17v3.25M3.75 10H7M3.75 14H7M17 10h3.25M17 14h3.25" />
    </>
  ),
  gauge: (
    <>
      <path d="M4.25 16.5a7.75 7.75 0 1 1 15.5 0" />
      <path d="M12 16.5l3.75-4.75" />
      <circle cx="12" cy="16.5" r="1" />
    </>
  ),
  lifebuoy: (
    <>
      <circle cx="12" cy="12" r="8.25" />
      <circle cx="12" cy="12" r="3.5" />
      <path d="M6.2 6.2l3.3 3.3M14.5 14.5l3.3 3.3M17.8 6.2l-3.3 3.3M9.5 14.5l-3.3 3.3" />
    </>
  ),
  bulb: (
    <path d="M9.5 18h5M10.25 21h3.5M12 3.5a5.75 5.75 0 0 0-3.4 10.4c.55.45.9 1.1.9 1.85V16h5v-.25c0-.75.35-1.4.9-1.85A5.75 5.75 0 0 0 12 3.5z" />
  ),
  book: (
    <path d="M12 6.75C10.25 5.4 7.75 4.75 4.5 4.75v13c3.25 0 5.75.65 7.5 2 1.75-1.35 4.25-2 7.5-2v-13c-3.25 0-5.75.65-7.5 2zM12 6.75v13" />
  ),
  stairs: <path d="M4 19.5h4.5V15H13v-4.5h4.5V6H20" />,
  eye: (
    <>
      <path d="M2.75 12S6.25 5.75 12 5.75 21.25 12 21.25 12 17.75 18.25 12 18.25 2.75 12 2.75 12z" />
      <circle cx="12" cy="12" r="2.75" />
    </>
  ),
  eyeOff: (
    <path d="M4 4l16 16M9.9 6.05A9.3 9.3 0 0 1 12 5.75c5.75 0 9.25 6.25 9.25 6.25a17.6 17.6 0 0 1-2.6 3.35M6.55 7.75A16.9 16.9 0 0 0 2.75 12S6.25 18.25 12 18.25a8.9 8.9 0 0 0 4.4-1.15M10.1 10.15a2.75 2.75 0 0 0 3.8 3.8" />
  ),
  moon: <path d="M19.25 14.75A7.75 7.75 0 0 1 9.25 4.75a7.75 7.75 0 1 0 10 10z" />,
  pause: <path d="M9 6.25v11.5M15 6.25v11.5" />,
  play: <path d="M8.5 5.75v12.5l10-6.25z" />,
  chart: <path d="M4.5 4.5v15h15M8 15l3.5-4.5 3 2.75L19 7.5" />,
  doc: (
    <path d="M14 3.75H7.25a1.5 1.5 0 0 0-1.5 1.5v13.5a1.5 1.5 0 0 0 1.5 1.5h9.5a1.5 1.5 0 0 0 1.5-1.5V8zM14 3.75V8h4.25M9 12.5h6M9 16h4" />
  ),
  review: (
    <path d="M19.25 12a7.25 7.25 0 0 1-12.6 4.9M4.75 12a7.25 7.25 0 0 1 12.6-4.9M17.6 3.75v3.6H14M6.4 20.25v-3.6H10" />
  ),
  shuffle: (
    <path d="M4 7.25h3.5c1.4 0 2.4.6 3.2 1.7l2.6 3.6c.8 1.1 1.8 1.7 3.2 1.7h3M4 16.75h3.5c1.4 0 2.4-.6 3.2-1.7M13.3 8.95c.8-1.1 1.8-1.7 3.2-1.7h3M17.25 5l2.25 2.25-2.25 2.25M17.25 12l2.25 2.25-2.25 2.25" />
  ),
  question: (
    <>
      <path d="M5.25 18.75V7.5a2.75 2.75 0 0 1 2.75-2.75h8A2.75 2.75 0 0 1 18.75 7.5v6a2.75 2.75 0 0 1-2.75 2.75H8.5z" />
      <path d="M10.1 9.1a1.95 1.95 0 1 1 2.55 1.85c-.4.15-.65.5-.65.9v.35M12 14.1v.15" />
    </>
  ),
  pencil: <path d="M4.75 19.25h3.5l10-10a2.47 2.47 0 0 0-3.5-3.5l-10 10zM13.5 7l3.5 3.5" />,
  key: (
    <>
      <circle cx="8" cy="15.5" r="3.75" />
      <path d="M10.75 12.75L19.5 4M16.25 7.25l2.5 2.5M13.75 9.75l1.75 1.75" />
    </>
  ),
  database: (
    <path d="M5 6.75C5 5.23 8.13 4 12 4s7 1.23 7 2.75-3.13 2.75-7 2.75-7-1.23-7-2.75zM5 6.75v10.5C5 18.77 8.13 20 12 20s7-1.23 7-2.75V6.75M5 12c0 1.52 3.13 2.75 7 2.75S19 13.52 19 12" />
  ),
  wrench: (
    <path d="M14.7 6.3a4.25 4.25 0 0 0-5.6 5.6l-5.1 5.1a1.5 1.5 0 0 0 0 2.1l.9.9a1.5 1.5 0 0 0 2.1 0l5.1-5.1a4.25 4.25 0 0 0 5.6-5.6l-2.6 2.6-2.4-.6-.6-2.4z" />
  ),
  flag: <path d="M6 20.5V4.5M6 5.25h11.25l-2.25 4.25 2.25 4.25H6" />,
  target: (
    <>
      <circle cx="12" cy="12" r="8.25" />
      <circle cx="12" cy="12" r="4.25" />
      <path d="M12 12h.01" />
    </>
  ),
  clipboard: (
    <path d="M9.25 4.25h5.5V7h-5.5zM9.25 5.5H7.5A1.75 1.75 0 0 0 5.75 7.25v12A1.75 1.75 0 0 0 7.5 21h9a1.75 1.75 0 0 0 1.75-1.75v-12A1.75 1.75 0 0 0 16.5 5.5h-1.75" />
  ),
  folder: (
    <path d="M3.75 7.5A1.75 1.75 0 0 1 5.5 5.75h3.75l2 2h7.25a1.75 1.75 0 0 1 1.75 1.75v7.75A1.75 1.75 0 0 1 18.5 19h-13a1.75 1.75 0 0 1-1.75-1.75z" />
  ),
  clock: (
    <>
      <circle cx="12" cy="12" r="8.25" />
      <path d="M12 7.5V12l3 2" />
    </>
  ),
  trash: <path d="M5 7h14M10 7V4.75h4V7M7 7l.9 12.25h8.2L17 7M10.25 10.5v5.5M13.75 10.5v5.5" />,
  circleDashed: (
    <path d="M12 3.75a8.25 8.25 0 0 1 2.4.36M18.2 6.6a8.25 8.25 0 0 1 1.7 3.4M20.1 14.1a8.25 8.25 0 0 1-1.9 3.3M14.4 19.9a8.25 8.25 0 0 1-4.8 0M5.8 17.4a8.25 8.25 0 0 1-1.9-3.3M4.1 10a8.25 8.25 0 0 1 1.7-3.4M9.6 4.1A8.25 8.25 0 0 1 12 3.75" />
  ),
  checkCircle: (
    <>
      <circle cx="12" cy="12" r="8.25" />
      <path d="M8.25 12.25l2.5 2.5 5-5.25" />
    </>
  ),
  chevronLeft: <path d="M14 7l-5 5 5 5" />,
  home: (
    <path d="M4.75 10.25L12 4.5l7.25 5.75v8.25a1.5 1.5 0 0 1-1.5 1.5h-3.5v-5.5h-4.5V20h-3.5a1.5 1.5 0 0 1-1.5-1.5z" />
  ),
  sidebar: (
    <>
      <rect x="3.75" y="4.75" width="16.5" height="14.5" rx="2.5" />
      <path d="M9.25 4.75v14.5" />
    </>
  ),
  more: (
    <g fill="currentColor" stroke="none">
      <circle cx="6.25" cy="12" r="1.4" />
      <circle cx="12" cy="12" r="1.4" />
      <circle cx="17.75" cy="12" r="1.4" />
    </g>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="3.75" />
      <path d="M12 3.5v1.75M12 18.75v1.75M3.5 12h1.75M18.75 12h1.75M5.99 5.99l1.24 1.24M16.77 16.77l1.24 1.24M5.99 18.01l1.24-1.24M16.77 7.23l1.24-1.24" />
    </>
  ),
  monitor: (
    <>
      <rect x="3.75" y="4.75" width="16.5" height="11.5" rx="2" />
      <path d="M9 20h6M12 16.25V20" />
    </>
  ),
  refresh: <path d="M19.25 12a7.25 7.25 0 1 1-2.1-5.1M19.5 4.75v3.5H16" />,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof paths;

/** Decorative stroke icon; it inherits the text colour and size and never changes an accessible name. */
export function Icon({ name }: { name: IconName }) {
  return (
    <svg
      className="icon"
      width="1em"
      height="1em"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {paths[name]}
    </svg>
  );
}

/** The app mark: the icon's descent curve and current point. */
export function LogoMark() {
  return (
    <svg className="logo-mark" viewBox="0 0 32 32" aria-hidden="true" focusable="false">
      <rect width="32" height="32" rx="7" />
      <path d="M8.5 9v14.5h15.5M9.75 10.75q6.25 17.5 12.5 0" />
      <circle cx="19.1" cy="17.3" r="1.75" />
    </svg>
  );
}
