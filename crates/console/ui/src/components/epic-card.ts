import { LitElement, html, css, nothing } from 'lit';
import { customElement, property, query } from 'lit/decorators.js';
import type { Task } from '../core/schema.js';
import { surface, controls } from '../styles/shared.js';
import './state-badge.js';

@customElement('epic-card')
export class EpicCard extends LitElement {
  static override styles = [surface, controls, css`
    :host { display: block; container-type: inline-size; }
    article { display: grid; gap: var(--space-3); }
    header { display: flex; justify-content: space-between; gap: var(--space-3); align-items: start; flex-wrap: wrap; }
    h3 { font-size: 1.05rem; font-weight: 700; text-wrap: balance; } h3 a { color: inherit; text-decoration: none; } h3 a:hover { text-decoration: underline; }
    .id { color: var(--fg-muted); font-family: var(--mono); font-size: .8rem; }
    .progress { display: grid; gap: var(--space-1); }
    .bar { block-size: .5rem; border-radius: 999px; background: var(--line); overflow: clip; }
    .bar > span { display: block; block-size: 100%; inline-size: var(--pct); background: linear-gradient(90deg, var(--accent), var(--ok)); transition: inline-size 600ms var(--ease-out); }
    output { font-variant-numeric: tabular-nums; color: var(--fg-muted); font-size: .85rem; }
    footer { display: flex; gap: var(--space-2); justify-content: end; align-items: center; }
    .why { font-size: .8rem; color: var(--fg-muted); }
    dialog { border: 1px solid var(--line); border-radius: var(--radius); padding: var(--space-6); background: var(--bg-elev); color: var(--fg); box-shadow: var(--shadow); max-inline-size: 28rem;
             transition: opacity 200ms var(--ease-out), translate 200ms var(--ease-out), overlay 200ms allow-discrete, display 200ms allow-discrete; }
    dialog::backdrop { background: oklch(0% 0 0 / 0.4); backdrop-filter: blur(4px); }
    dialog[open] { opacity: 1; translate: 0 0; @starting-style { opacity: 0; translate: 0 8px; } }
    dialog:not([open]) { opacity: 0; translate: 0 8px; }
    dialog form { display: grid; gap: var(--space-3); }
    dialog menu { display: flex; gap: var(--space-2); justify-content: end; margin: 0; padding: 0; list-style: none; }
    @container (max-width: 28rem) { header { flex-direction: column; } }
  `];

  @property({ attribute: false }) task!: Task;
  /** When set, the title links to the epic page. */
  @property() rig = '';
  @property({ type: Boolean }) pending = false;
  @property({ type: Boolean }) allowed = true;
  @property() reason = '';
  @query('dialog') private dialog?: HTMLDialogElement;

  override render() {
    const f = this.task.metadata.factory;
    const pct = f.tasks === 0 ? 0 : Math.round((f.closed / f.tasks) * 100);
    const terminal = ['TASK_STATE_COMPLETED', 'TASK_STATE_FAILED', 'TASK_STATE_CANCELED', 'TASK_STATE_REJECTED'].includes(this.task.status.state);
    return html`<article class="surface" aria-labelledby="t-${this.task.id}">
      <header>
        <div>
          <h3 id="t-${this.task.id}">${this.rig === '' ? (f.title === '' ? this.task.id : f.title) : html`<a href="/rigs/${this.rig}/epics/${this.task.id}">${f.title === '' ? this.task.id : f.title}</a>`}</h3>
          <span class="id">${this.task.id}</span>
        </div>
        <state-badge .state=${this.task.status.state}></state-badge>
      </header>
      <div class="progress">
        <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=${pct} style="--pct:${pct}%"><span></span></div>
        <output>${f.closed}/${f.tasks} tasks landed${f.incidents > 0 ? html` · <strong>${f.incidents} need${f.incidents === 1 ? 's' : ''} you</strong>` : ''}</output>
      </div>
      ${terminal ? nothing : html`<footer>
        ${this.allowed ? nothing : html`<span class="why">${this.reason}</span>`}
        <button type="button" class="danger" ?disabled=${!this.allowed || this.pending} @click=${() => this.dialog?.showModal()}>${this.pending ? 'Stopping…' : 'Stop'}</button>
      </footer>
      <dialog aria-labelledby="d-${this.task.id}">
        <form method="dialog" @submit=${this.confirm}>
          <strong id="d-${this.task.id}">Stop ${this.task.id}?</strong>
          <p>Every open task under this epic is closed. Work already landed on main stays.</p>
          <menu>
            <li><button type="button" @click=${() => this.dialog?.close()}>Keep going</button></li>
            <li><button type="submit" class="danger" value="stop">Stop the epic</button></li>
          </menu>
        </form>
      </dialog>`}
    </article>`;
  }

  private readonly confirm = (): void => {
    this.dispatchEvent(new CustomEvent('stop-epic', { detail: { id: this.task.id }, bubbles: true, composed: true }));
  };
}

declare global {
  interface HTMLElementTagNameMap { 'epic-card': EpicCard; }
}
