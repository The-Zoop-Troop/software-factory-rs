import { LitElement, html, css } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { messageText, type Task } from '../core/schema.js';
import { surface } from '../styles/shared.js';
import './state-badge.js';

/** A queued plan: shows the planner's latest progress line until the epic exists. */
@customElement('request-card')
export class RequestCard extends LitElement {
  static override styles = [surface, css`
    article { display: grid; gap: var(--space-2); border-inline-start: 4px solid var(--info); }
    header { display: flex; justify-content: space-between; gap: var(--space-2); flex-wrap: wrap; }
    h3 { font-size: 1rem; font-weight: 700; }
    .id { font-family: var(--mono); font-size: .8rem; color: var(--fg-muted); }
    .progress { display: inline-flex; gap: .5rem; align-items: center; color: var(--fg-muted); font-size: .9rem; }
    .spinner { inline-size: .9em; block-size: .9em; border-radius: 50%; border: 2px solid var(--accent-soft); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
    .failed { color: var(--danger); }
    @keyframes spin { to { transform: rotate(360deg); } }
  `];

  @property({ attribute: false }) task!: Task;

  override render() {
    const f = this.task.metadata.factory;
    const state = this.task.status.state;
    const line = messageText(this.task.status.message);
    return html`<article class="surface" aria-labelledby="r-${this.task.id}">
      <header>
        <div><h3 id="r-${this.task.id}">${f.title}</h3><span class="id">${this.task.id}</span></div>
        <state-badge .state=${state}></state-badge>
      </header>
      ${state === 'TASK_STATE_SUBMITTED'
        ? html`<p class="progress" aria-live="polite"><span class="spinner" aria-hidden="true"></span>${line === '' ? 'queued for the planner' : line}</p>`
        : state === 'TASK_STATE_FAILED'
          ? html`<p class="failed">${line}</p>`
          : html`<p class="progress">epic <a href="#${f.epic ?? ''}">${f.epic ?? ''}</a> created</p>`}
    </article>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'request-card': RequestCard; }
}
