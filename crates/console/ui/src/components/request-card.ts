import { LitElement, html, css, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { loadBeadDetail } from '../actions.js';
import { messageText, type RigName, type Task } from '../core/schema.js';
import { beadDetails } from '../state/detail.js';
import { surface } from '../styles/shared.js';
import './state-badge.js';

const CONTRACTS_MARKER = '\n## Upstream contracts';

/** Split a request's text into the human's plan and the injected contract sections. */
export const splitPlan = (text: string): { readonly plan: string; readonly contracts: ReadonlyArray<{ readonly need: string; readonly text: string }> } => {
  const at = text.indexOf(CONTRACTS_MARKER);
  if (at === -1) return { plan: text, contracts: [] };
  const plan = text.slice(0, at).trimEnd();
  const rest = text.slice(at);
  const contracts = [...rest.matchAll(/^### (\S+)\n([\s\S]*?)(?=\n### |$)/gm)].map((m) => ({
    need: m[1] ?? '',
    text: (m[2] ?? '').trim(),
  }));
  return { plan, contracts };
};

/** A queued plan: shows the planner's latest progress line until the epic exists. */
@customElement('request-card')
export class RequestCard extends SignalWatcher(LitElement) {
  static override styles = [surface, css`
    article { display: grid; gap: var(--space-2); border-inline-start: 4px solid var(--info); }
    header { display: flex; justify-content: space-between; gap: var(--space-2); flex-wrap: wrap; }
    h3 { font-size: 1rem; font-weight: 700; }
    .id { font-family: var(--mono); font-size: .8rem; color: var(--fg-muted); }
    .progress { display: inline-flex; gap: .5rem; align-items: center; color: var(--fg-muted); font-size: .9rem; }
    .spinner { inline-size: .9em; block-size: .9em; border-radius: 50%; border: 2px solid var(--accent-soft); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
    .failed { color: var(--danger); }
    @keyframes spin { to { transform: rotate(360deg); } }
    button.expand { border: none; background: none; padding: 0; color: var(--accent-strong); cursor: pointer; font: inherit; font-size: .85rem; inline-size: fit-content; }
    .plan pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: .85rem; }
    details.contract { border: 1px solid var(--line); border-radius: var(--radius-sm); padding: var(--space-1) var(--space-2); }
    details.contract summary { cursor: pointer; font-weight: 600; font-family: var(--mono); font-size: .85rem; }
  `];

  @property({ attribute: false }) task!: Task;
  @property() rig = '';
  @state() private expanded = false;

  override render() {
    const f = this.task.metadata.factory;
    const state = this.task.status.state;
    const line = messageText(this.task.status.message);
    return html`<article class="surface" aria-labelledby="r-${this.task.id}">
      <header>
        <div><h3 id="r-${this.task.id}">${f.title}</h3><span class="id">${this.task.id}</span></div>
        <state-badge .state=${state}></state-badge>
      </header>
      ${state === 'TASK_STATE_SUBMITTED' && f.waiting
        ? html`<p class="progress waiting">after ${f.needs.join(', ')} — waiting for them to land</p>`
        : state === 'TASK_STATE_SUBMITTED'
        ? html`<p class="progress" aria-live="polite"><span class="spinner" aria-hidden="true"></span>${line === '' ? 'queued for the planner' : line}</p>`
        : state === 'TASK_STATE_FAILED'
          ? html`<p class="failed">${line}</p>`
          : html`<p class="progress">epic <a href="#${f.epic ?? ''}">${f.epic ?? ''}</a> created</p>`}
      ${this.rig === '' ? nothing : html`<button class="expand" @click=${this.toggle}>${this.expanded ? 'hide plan text' : 'show plan text'}</button>`}
      ${this.expanded ? this.renderPlan() : nothing}
    </article>`;
  }

  /** The full submitted text, with injected upstream contracts as collapsible sections. */
  private renderPlan() {
    const d = beadDetails.get()[`${this.rig}/${this.task.id}`];
    if (d === undefined) return html`<p class="progress">loading plan text…</p>`;
    const { plan, contracts } = splitPlan(d.description);
    return html`<div class="plan" style="display:grid; gap: var(--space-2)">
      <pre>${plan}</pre>
      ${contracts.map((c) => html`<details class="contract"><summary>upstream contract — ${c.need}</summary><pre>${c.text}</pre></details>`)}
    </div>`;
  }

  private readonly toggle = (): void => {
    this.expanded = !this.expanded;
    if (this.expanded) void loadBeadDetail(this.rig as RigName, this.task.id);
  };
}

declare global {
  interface HTMLElementTagNameMap { 'request-card': RequestCard; }
}
