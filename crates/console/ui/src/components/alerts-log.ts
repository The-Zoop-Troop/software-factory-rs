import { LitElement, html, css } from 'lit';
import { customElement } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { alerts, str } from '../state/events.js';

/** Webhook / chat deliveries the console made during this session. */
@customElement('alerts-log')
export class AlertsLog extends SignalWatcher(LitElement) {
  static override styles = css`
    :host { display: block; }
    ol { list-style: none; margin: 0; padding: 0; display: grid; gap: .35rem; }
    li { display: grid; grid-template-columns: auto 1fr; gap: .6rem; align-items: baseline; font-size: .9rem; padding: .35rem .6rem; border-radius: var(--radius-sm); border: 1px solid var(--line); background: var(--bg-elev); }
    li::before { content: '✉'; color: var(--info); } li.failed::before { content: '✕'; color: var(--danger); }
    time { color: var(--fg-muted); font-family: var(--mono); font-size: .75rem; margin-inline-start: .5rem; }
    .empty { color: var(--fg-muted); font-size: .9rem; }
  `;

  override render() {
    const rows = alerts.get();
    if (rows.length === 0) return html`<p class="empty">No alerts delivered while this session has been connected.</p>`;
    return html`<ol aria-live="polite">${repeat(rows, (f) => `${f.rig}:${String(f.cursor)}:${String(f.record.at)}`, (f) => {
      const failed = f.record['action'] === 'alert-failed';
      const at = typeof f.record.at === 'number' ? new Date(f.record.at * 1000) : new Date(f.record.at);
      return html`<li class=${failed ? 'failed' : ''}><span>${str(f.record['detail'])}<time datetime=${at.toISOString()}>${f.rig} · ${at.toLocaleTimeString()}</time></span></li>`;
    })}</ol>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'alerts-log': AlertsLog; }
}
