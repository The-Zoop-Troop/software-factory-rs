import { LitElement, html, css } from 'lit';
import { customElement } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { EMPTY, PAGE } from '../copy.js';
import { summaries } from '../state/rigs.js';
import { connection } from '../state/session.js';
import { surface, badges, controls } from '../styles/shared.js';

@customElement('overview-page')
export class OverviewPage extends SignalWatcher(LitElement) {
  static override styles = [surface, badges, controls, css`
    :host { display: block; }
    h1 { font-size: 1.6rem; font-weight: 800; letter-spacing: -0.01em; margin-block-end: var(--space-1); text-wrap: balance; }
    .page-desc { margin-block-end: var(--space-4); }
    .grid { display: grid; gap: var(--space-4); grid-template-columns: repeat(auto-fill, minmax(min(100%, 18rem), 1fr)); }
    a.surface { display: grid; gap: var(--space-2); text-decoration: none; color: inherit; view-transition-name: var(--vt); }
    .name { font-size: 1.2rem; font-weight: 800; font-family: var(--mono); }
    .stats { display: flex; gap: var(--space-2); flex-wrap: wrap; }
    .empty { color: var(--fg-muted); }
  `];

  override render() {
    const rows = summaries.get();
    return html`<h1>Rigs</h1>
      <p class="page-desc">${PAGE.overview.desc}</p>
      ${rows.length === 0
        ? html`<p class="empty">${connection.get() === 'online' ? EMPTY.rigsNone : EMPTY.rigsOffline}</p>`
        : html`<div class="grid">
          ${repeat(rows, (r) => r.rig, (r) => html`
            <a class="surface" href="/rigs/${r.rig}" style="--vt: rig-${r.rig}">
              <span class="name">${r.rig}</span>
              <span class="stats">
                ${r.unavailable !== null ? html`<span class="badge danger" title=${r.unavailable}>unavailable</span>` : html`<span class="badge info">${r.epics} epic${r.epics === 1 ? '' : 's'}</span>`}
                ${r.working > 0 ? html`<span class="badge working">${r.working} working</span>` : ''}
                ${r.attention > 0 ? html`<span class="badge warn">${r.attention} need you</span>` : ''}
                ${r.done > 0 ? html`<span class="badge ok">${r.done} done</span>` : ''}
              </span>
            </a>`)}
        </div>`}`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'overview-page': OverviewPage; }
}
