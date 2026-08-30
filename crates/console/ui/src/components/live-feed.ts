import { LitElement, html, css } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { describe, recent, streamStatus } from '../state/events.js';

/** The rig's recent events, newest first. */
@customElement('live-feed')
export class LiveFeed extends SignalWatcher(LitElement) {
  static override styles = css`
    :host { display: block; }
    ol { list-style: none; margin: 0; padding: 0; display: grid; gap: .35rem; }
    li { display: grid; grid-template-columns: auto 1fr; gap: .6rem; align-items: baseline; font-size: .9rem;
         padding: .35rem .6rem; border-radius: var(--radius-sm); background: var(--bg-elev); border: 1px solid var(--line);
         transition: opacity 300ms var(--ease-out), translate 300ms var(--ease-out); @starting-style { opacity: 0; translate: -6px 0; } }
    li::before { content: ''; inline-size: .5rem; block-size: .5rem; border-radius: 50%; background: var(--tone, var(--fg-muted)); }
    .success { --tone: var(--ok); } .warning { --tone: var(--warn); } .danger { --tone: var(--danger); } .info { --tone: var(--info); }
    time { color: var(--fg-muted); font-family: var(--mono); font-size: .75rem; margin-inline-start: .5rem; }
    .empty { color: var(--fg-muted); font-size: .9rem; }
  `;

  @property() rig = '';
  @property({ type: Number }) limit = 12;

  override render() {
    const rows = recent.get().filter((f) => f.rig === this.rig).slice(-this.limit).reverse();
    const status = streamStatus.get();
    if (rows.length === 0) return html`<p class="empty">${status === 'live' ? 'Listening — events appear here as the rig works.' : 'No events yet.'}</p>`;
    return html`<ol aria-live="polite">${repeat(rows, (f) => `${f.rig}:${String(f.cursor)}:${f.record.kind}:${String(f.record.at)}`, (f) => {
      const line = describe(f) ?? { title: `${f.record.actor}: ${f.record.kind}`, tone: 'info' as const };
      const at = typeof f.record.at === 'number' ? new Date(f.record.at * 1000) : new Date(f.record.at);
      return html`<li class=${line.tone}><span>${line.title}<time datetime=${at.toISOString()}>${at.toLocaleTimeString()}</time></span></li>`;
    })}</ol>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'live-feed': LiveFeed; }
}
