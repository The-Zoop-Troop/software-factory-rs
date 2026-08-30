import { LitElement, html, css } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import type { Task } from '../core/schema.js';
import { surface, controls } from '../styles/shared.js';
import './state-badge.js';

@customElement('epic-card')
export class EpicCard extends LitElement {
  static override styles = [surface, controls, css`
    :host { display: block; container-type: inline-size; }
    article { display: grid; gap: var(--space-3); }
    header { display: flex; justify-content: space-between; gap: var(--space-3); align-items: start; flex-wrap: wrap; }
    h3 { font-size: 1.05rem; font-weight: 700; text-wrap: balance; }
    .id { color: var(--fg-muted); font-family: var(--mono); font-size: .8rem; }
    .progress { display: grid; gap: var(--space-1); }
    .bar { block-size: .5rem; border-radius: 999px; background: var(--line); overflow: clip; }
    .bar > span { display: block; block-size: 100%; inline-size: var(--pct); background: linear-gradient(90deg, var(--accent), var(--ok)); transition: inline-size 600ms var(--ease-out); }
    output { font-variant-numeric: tabular-nums; color: var(--fg-muted); font-size: .85rem; }
    footer { display: flex; gap: var(--space-2); justify-content: end; }
    @container (max-width: 28rem) { header { flex-direction: column; } }
  `];

  @property({ attribute: false }) task!: Task;

  override render() {
    const f = this.task.metadata.factory;
    const pct = f.tasks === 0 ? 0 : Math.round((f.closed / f.tasks) * 100);
    const terminal = ['TASK_STATE_COMPLETED', 'TASK_STATE_FAILED', 'TASK_STATE_CANCELED', 'TASK_STATE_REJECTED'].includes(this.task.status.state);
    return html`<article class="surface" aria-labelledby="t-${this.task.id}">
      <header>
        <div>
          <h3 id="t-${this.task.id}">${f.title === '' ? this.task.id : f.title}</h3>
          <span class="id">${this.task.id}</span>
        </div>
        <state-badge .state=${this.task.status.state}></state-badge>
      </header>
      <div class="progress">
        <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=${pct} style="--pct:${pct}%"><span></span></div>
        <output>${f.closed}/${f.tasks} tasks landed${f.incidents > 0 ? html` · <strong>${f.incidents} need${f.incidents === 1 ? 's' : ''} you</strong>` : ''}</output>
      </div>
      ${terminal ? '' : html`<footer>
        <button type="button" class="danger" @click=${() => this.dispatchEvent(new CustomEvent('stop-epic', { detail: { id: this.task.id }, bubbles: true, composed: true }))}>Stop</button>
      </footer>`}
    </article>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'epic-card': EpicCard; }
}
