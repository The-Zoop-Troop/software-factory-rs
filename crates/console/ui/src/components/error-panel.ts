import { LitElement, html, css } from 'lit';
import { customElement } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { lastError } from '../state/session.js';
import { controls } from '../styles/shared.js';

@customElement('error-panel')
export class ErrorPanel extends SignalWatcher(LitElement) {
  static override styles = [controls, css`
    :host { display: block; }
    .panel {
      display: grid; gap: .25rem; padding: .9rem 1.1rem; margin-block: var(--space-4);
      border-radius: var(--radius); border: 1px solid color-mix(in oklch, var(--danger) 45%, transparent);
      background: color-mix(in oklch, var(--danger) 10%, var(--bg-elev));
      transition: opacity 250ms var(--ease-out); @starting-style { opacity: 0; }
    }
    strong { color: var(--danger); } .recovery { color: var(--fg-muted); font-size: .9rem; }
    .actions { display: flex; gap: var(--space-2); }
  `];

  override render() {
    const e = lastError.get();
    if (e === null) return html``;
    return html`<div class="panel" role="alert">
      <strong>${e.title}</strong>
      <span>${e.detail}</span>
      <span class="recovery">${e.recovery}</span>
      <div class="actions">
        <button type="button" @click=${() => this.dispatchEvent(new CustomEvent('recover', { detail: { action: 'retry' }, bubbles: true, composed: true }))}>Try again</button>
        ${e.title === 'Not signed in' ? html`<button type="button" class="primary" @click=${() => this.dispatchEvent(new CustomEvent('recover', { detail: { action: 'sign-in' }, bubbles: true, composed: true }))}>Enter a token</button>` : ''}
        <button type="button" @click=${() => { lastError.set(null); }}>Dismiss</button>
      </div>
    </div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'error-panel': ErrorPanel; }
}
