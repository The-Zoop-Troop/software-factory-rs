// Styles shared by components (constructable stylesheets, one instance per document).
import { css } from 'lit';

export const surface = css`
  .surface {
    background: var(--bg-elev);
    backdrop-filter: blur(14px) saturate(1.4);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: var(--space-4);
    transition: box-shadow 300ms var(--ease-out), translate 300ms var(--ease-out);
    @starting-style { opacity: 0; translate: 0 8px; }
  }
  @media (hover: hover) {
    .surface:hover { box-shadow: var(--shadow-raised); }
  }
`;

export const controls = css`
  button, .button {
    inline-size: fit-content;
    padding: 0.55rem 1rem;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--bg-elev);
    color: var(--fg);
    cursor: pointer;
    font-weight: 600;
    display: inline-flex; align-items: center; gap: var(--space-2);
    touch-action: manipulation;
    transition: background 200ms var(--ease-out), translate 200ms var(--ease-out), box-shadow 200ms var(--ease-out);
  }
  @media (hover: hover) {
    button:hover:not(:disabled) { translate: 0 -1px; box-shadow: var(--shadow); }
    button.primary:hover:not(:disabled) { background: var(--accent-strong); }
  }
  button:active:not(:disabled) { translate: 0 0; }
  button:disabled { opacity: 0.55; cursor: not-allowed; }
  button.primary { background: var(--accent); color: white; border-color: transparent; }
  button.danger { color: var(--danger); border-color: color-mix(in oklch, var(--danger) 40%, transparent); }
  input, textarea {
    inline-size: 100%;
    padding: 0.6rem 0.8rem;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: light-dark(white, oklch(14% 0.008 var(--hue)));
    color: var(--fg);
  }
  input:user-invalid, textarea:user-invalid { border-color: var(--danger); }
  label { display: grid; gap: var(--space-1); font-size: 0.9rem; color: var(--fg-muted); }
  .muted { color: var(--fg-muted); }
  .mono { font-family: var(--mono); font-size: 0.85em; }
  .page-desc {
    color: var(--fg-muted); max-inline-size: 70ch; text-wrap: pretty; font-size: 0.9rem;
    padding: var(--space-2) var(--space-3);
    border-inline-start: 3px solid color-mix(in oklch, var(--accent) 45%, transparent);
    background: color-mix(in oklch, var(--accent) 5%, transparent);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }
  .empty { color: var(--fg-muted); font-size: 0.95rem; text-wrap: pretty; padding: var(--space-4); border: 1px dashed var(--line); border-radius: var(--radius); }
`;

export const badges = css`
  .badge {
    display: inline-flex; align-items: center; gap: 0.35em;
    padding: 0.15rem 0.6rem; border-radius: 999px; font-size: 0.78rem; font-weight: 700; letter-spacing: 0.02em;
    text-transform: uppercase; background: var(--accent-soft); color: light-dark(var(--accent-strong), oklch(from var(--accent) 85% c h));
  }
  .badge::before { content: ''; inline-size: 0.5em; block-size: 0.5em; border-radius: 50%; background: currentColor; }
  .badge.working::before { animation: pulse 1.4s infinite var(--ease-out); }
  .badge.ok { background: color-mix(in oklch, var(--ok) 18%, transparent); color: light-dark(oklch(from var(--ok) 38% c h), oklch(from var(--ok) 85% c h)); }
  .badge.warn { background: color-mix(in oklch, var(--warn) 22%, transparent); color: light-dark(oklch(from var(--warn) 40% c h), oklch(from var(--warn) 88% c h)); }
  .badge.danger { background: color-mix(in oklch, var(--danger) 16%, transparent); color: light-dark(var(--danger), oklch(from var(--danger) 80% c h)); }
  .badge.info { background: color-mix(in oklch, var(--info) 18%, transparent); color: light-dark(oklch(from var(--info) 38% c h), oklch(from var(--info) 85% c h)); }
  .badge.working { color: light-dark(var(--accent-strong), oklch(from var(--accent) 85% c h)); }
  @keyframes pulse { 0%, 100% { transform: scale(1); opacity: 1; } 50% { transform: scale(1.6); opacity: 0.5; } }
`;
