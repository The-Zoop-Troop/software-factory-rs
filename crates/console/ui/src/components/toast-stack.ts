import { LitElement, html, css } from 'lit';
import { customElement } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { dismiss, notices } from '../state/notices.js';

@customElement('toast-stack')
export class ToastStack extends SignalWatcher(LitElement) {
  static override styles = css`
    :host { position: fixed; inset-inline-end: 1rem; inset-block-end: 1rem; display: grid; gap: .5rem; z-index: 50; max-inline-size: min(28rem, 90vw); }
    .toast {
      display: grid; grid-template-columns: 1fr auto; gap: .25rem .75rem; align-items: start;
      padding: .75rem 1rem; border-radius: var(--radius); border: 1px solid var(--line);
      background: var(--bg-elev); backdrop-filter: blur(14px); box-shadow: var(--shadow);
      border-inline-start: 4px solid var(--tone);
      transition: opacity 250ms var(--ease-out), translate 250ms var(--ease-out);
      @starting-style { opacity: 0; translate: 1rem 0; }
    }
    .info { --tone: var(--info); } .success { --tone: var(--ok); } .warning { --tone: var(--warn); } .danger { --tone: var(--danger); }
    strong { font-weight: 700; } p { grid-column: 1; color: var(--fg-muted); font-size: .9rem; }
    button { grid-row: 1 / span 2; border: 0; background: transparent; color: var(--fg-muted); cursor: pointer; font-size: 1.1rem; }
  `;

  override render() {
    return html`<div role="status" aria-live="polite">
      ${repeat(notices.get(), (n) => n.id, (n) => html`
        <div class="toast ${n.tone}">
          <strong>${n.title}</strong>
          <button type="button" aria-label="Dismiss" @click=${() => { dismiss(n.id); }}>×</button>
          ${n.detail === undefined ? '' : html`<p>${n.detail}</p>`}
        </div>`)}
    </div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'toast-stack': ToastStack; }
}
