import { LitElement, html, css } from 'lit';
import { customElement, property, state, query } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { applyOption, loadHistory, pending, resolveItem, refreshRig, stopEpic, submitPlan } from '../actions.js';
import type { AttentionOption } from '../core/schema.js';
import { can, whyNot } from '../state/session.js';
import type { RigName } from '../core/schema.js';
import { currentRig, historyByRig, isEpic, isRequest, needsHuman, tasksByRig } from '../state/rigs.js';
import { controls } from '../styles/shared.js';
import '../components/epic-card.js';
import '../components/state-badge.js';
import '../components/plan-form.js';
import '../components/inbox-item.js';
import '../components/request-card.js';
import '../components/live-feed.js';
import '../components/alerts-log.js';
import type { PlanForm } from '../components/plan-form.js';

@customElement('rig-page')
export class RigPage extends SignalWatcher(LitElement) {
  static override styles = [controls, css`
    details.completed summary { cursor: pointer; list-style: none; display: flex; align-items: center; gap: var(--space-2); }
    details.completed summary::-webkit-details-marker { display: none; }
    details.completed summary::before { content: '▸'; color: var(--fg-muted); transition: transform 150ms; }
    details.completed[open] summary::before { transform: rotate(90deg); }
    details.completed summary h2 { margin: 0; }
    details.completed table { width: 100%; border-collapse: collapse; }
    details.completed th, details.completed td { text-align: left; padding: var(--space-2) var(--space-3); border-block-end: 1px solid var(--line); }
    details.completed .num { text-align: right; font-variant-numeric: tabular-nums; }
    details.completed .mono { font-family: var(--mono); font-size: 0.85em; }
    details.completed .muted { color: var(--fg-muted); }

    :host { display: grid; gap: var(--space-6); }
    header { display: flex; align-items: baseline; gap: var(--space-3); flex-wrap: wrap; }
    h1 { font-size: 1.6rem; font-weight: 800; font-family: var(--mono); view-transition-name: var(--vt); }
    h2 { font-size: 1.1rem; font-weight: 700; color: var(--fg-muted); margin-block-end: var(--space-3); display: flex; gap: var(--space-2); align-items: center; }
    .count { font-size: .8rem; background: var(--accent-soft); color: var(--accent-strong); border-radius: 999px; padding: 0 .6rem; }
    .grid { display: grid; gap: var(--space-4); grid-template-columns: repeat(auto-fill, minmax(min(100%, 22rem), 1fr)); }
    .empty { color: var(--fg-muted); }
  `];

  @property() rig = '';
  @state() private planning = false;
  @state() private resolving: string | null = null;
  @query('plan-form') private form?: PlanForm;

  override connectedCallback(): void {
    super.connectedCallback();
    currentRig.set(this.rig as RigName);
    void refreshRig(this.rig as RigName);
    void loadHistory(this.rig as RigName);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    currentRig.set(null);
  }

  override render() {
    const tasks = tasksByRig.get()[this.rig] ?? [];
    const epics = tasks.filter(isEpic);
    const requests = tasks.filter(isRequest);
    const inbox = tasks.filter((t) => !isEpic(t) && !isRequest(t) && needsHuman(t));
    const done = (historyByRig.get()[this.rig] ?? []).filter(isEpic);
    return html`
      <header><a href="/">Rigs</a><span class="muted">/</span><h1 style="--vt: rig-${this.rig}">${this.rig}</h1></header>
      <plan-form ?pending=${this.planning} .allowed=${can(this.rig, 'plan')} .reason=${whyNot(this.rig, 'plan')} @submit-plan=${this.onPlan}></plan-form>
      ${inbox.length === 0 ? '' : html`<section aria-labelledby="inbox-h">
        <h2 id="inbox-h">Needs you <span class="count">${inbox.length}</span></h2>
        <div class="grid">${repeat(inbox, (t) => t.id, (t) => html`<inbox-item .task=${t} ?pending=${pending.get().has(t.id) || this.resolving === t.id} .allowed=${can(this.rig, 'resolve')} .reason=${whyNot(this.rig, 'resolve')} @resolve-item=${this.onResolve} @apply-option=${this.onOption}></inbox-item>`)}</div>
      </section>`}
      ${requests.length === 0 ? '' : html`<section aria-labelledby="req-h">
        <h2 id="req-h">Planning <span class="count">${requests.length}</span></h2>
        <div class="grid">${repeat(requests, (t) => t.id, (t) => html`<request-card .task=${t}></request-card>`)}</div>
      </section>`}
      <section aria-labelledby="epics-h">
        <h2 id="epics-h">Epics <span class="count">${epics.length}</span></h2>
        ${epics.length === 0
          ? html`<p class="empty">Nothing in flight. Submit a plan above.</p>`
          : html`<div class="grid">${repeat(epics, (t) => t.id, (t) => html`<epic-card .task=${t} rig=${this.rig} ?pending=${pending.get().has(t.id)} .allowed=${can(this.rig, 'plan')} .reason=${whyNot(this.rig, 'plan')} @stop-epic=${this.onStop}></epic-card>`)}</div>`}
      </section>
      ${done.length === 0 ? '' : html`<section aria-labelledby="done-h">
        <details class="completed">
          <summary><h2 id="done-h">Completed <span class="count">${done.length}</span></h2></summary>
          <div class="surface"><table>
            <thead><tr><th>Epic</th><th class="num">Tasks</th><th class="num">Tokens</th><th>State</th></tr></thead>
            <tbody>${repeat(done, (t) => t.id, (t) => html`<tr>
              <td><a href="/rigs/${this.rig}/epics/${t.id}"><strong>${t.metadata.factory.title}</strong></a><br><span class="mono muted">${t.id}</span></td>
              <td class="num">${t.metadata.factory.closed}/${t.metadata.factory.tasks}</td>
              <td class="num">${Math.round(t.metadata.factory.children.reduce((s, c) => s + c.tokens, 0) / 1000)}k</td>
              <td><state-badge .state=${t.status.state}></state-badge></td>
            </tr>`)}</tbody>
          </table></div>
        </details>
      </section>`}
      <section aria-labelledby="feed-h">
        <h2 id="feed-h">Live</h2>
        <live-feed rig=${this.rig}></live-feed>
      </section>
      <section aria-labelledby="alerts-h">
        <h2 id="alerts-h">Alerts sent</h2>
        <alerts-log></alerts-log>
      </section>`;
  }

  private readonly onPlan = async (e: CustomEvent<{ text: string }>): Promise<void> => {
    this.planning = true;
    const ok = await submitPlan(this.rig as RigName, e.detail.text);
    this.planning = false;
    if (ok) this.form?.clear();
  };

  private readonly onResolve = async (e: CustomEvent<{ id: string; note: string }>): Promise<void> => {
    this.resolving = e.detail.id;
    await resolveItem(this.rig as RigName, e.detail.id, e.detail.note);
    this.resolving = null;
  };

  private readonly onOption = async (e: CustomEvent<{ id: string; option: AttentionOption; note: string }>): Promise<void> => {
    await applyOption(this.rig as RigName, e.detail.id, e.detail.option, e.detail.note);
  };

  private readonly onStop = async (e: CustomEvent<{ id: string }>): Promise<void> => {
    await stopEpic(this.rig as RigName, e.detail.id);
  };
}

declare global {
  interface HTMLElementTagNameMap { 'rig-page': RigPage; }
}
