import { LitElement, html, css, nothing } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { applyOption, pending, refreshRig, stopEpic } from '../actions.js';
import type { AttentionOption, Child, RigName } from '../core/schema.js';
import { attentionOf } from '../core/schema.js';
import { describe, forEpic, latestProgress } from '../state/events.js';
import { currentRig, taskById, tasksByRig } from '../state/rigs.js';
import { can, whyNot } from '../state/session.js';
import { badges, controls, surface } from '../styles/shared.js';
import '../components/epic-card.js';
import '../components/inbox-item.js';

const stateTone = (s: string): string =>
  s === 'closed' ? 'ok' : s === 'incident' ? 'danger' : s === 'open' ? 'info' : 'working';

@customElement('epic-page')
export class EpicPage extends SignalWatcher(LitElement) {
  static override styles = [surface, controls, badges, css`
    :host { display: grid; gap: var(--space-6); }
    header { display: flex; align-items: baseline; gap: var(--space-3); flex-wrap: wrap; }
    header a { color: inherit; }
    h1 { font-size: 1.5rem; font-weight: 800; font-family: var(--mono); }
    h2 { font-size: 1.1rem; font-weight: 700; color: var(--fg-muted); margin-block-end: var(--space-3); }
    .layout { display: grid; gap: var(--space-6); grid-template-columns: minmax(0, 2fr) minmax(0, 1fr); }
    @media (max-width: 60rem) { .layout { grid-template-columns: 1fr; } }
    table { inline-size: 100%; border-collapse: collapse; font-size: .92rem; }
    th, td { text-align: start; padding: .5rem .6rem; border-block-end: 1px solid var(--line); vertical-align: top; }
    th { color: var(--fg-muted); font-weight: 600; font-size: .8rem; text-transform: uppercase; letter-spacing: .04em; }
    td.num { font-variant-numeric: tabular-nums; text-align: end; }
    .timeline { list-style: none; margin: 0; padding: 0; display: grid; gap: .5rem; position: relative; }
    .timeline::before { content: ''; position: absolute; inset-block: .4rem; inset-inline-start: .45rem; inline-size: 2px; background: var(--line); }
    .timeline li { display: grid; grid-template-columns: 1rem 1fr; gap: .6rem; align-items: baseline; }
    .timeline li::before { content: ''; inline-size: .6rem; block-size: .6rem; border-radius: 50%; background: var(--tone, var(--fg-muted)); border: 2px solid var(--bg); box-shadow: 0 0 0 2px var(--tone, var(--line)); z-index: 1; }
    .success { --tone: var(--ok); } .warning { --tone: var(--warn); } .danger { --tone: var(--danger); } .info { --tone: var(--info); }
    .timeline time { color: var(--fg-muted); font-family: var(--mono); font-size: .75rem; margin-inline-start: .5rem; }
    .empty { color: var(--fg-muted); }
  `];

  @property() rig = '';
  @property() id = '';

  override connectedCallback(): void {
    super.connectedCallback();
    currentRig.set(this.rig as RigName);
    if (taskById(this.rig, this.id) === undefined) void refreshRig(this.rig as RigName);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    currentRig.set(null);
  }

  override render() {
    const epic = taskById(this.rig, this.id);
    const inbox = (tasksByRig.get()[this.rig] ?? []).filter((t) => attentionOf(t.status.message)?.epicId === this.id || (t.contextId === this.id && t.id !== this.id && t.metadata.factory.kind !== 'epic'));
    const all = forEpic(this.rig, this.id);
    const working = latestProgress(all);
    const events = all.slice(-40).reverse();
    return html`
      <header><a href="/">Rigs</a><span class="muted">/</span><a href="/rigs/${this.rig}">${this.rig}</a><span class="muted">/</span><h1>${this.id}</h1></header>
      ${epic === undefined ? html`<p class="empty">Loading ${this.id}…</p>` : html`
        <epic-card .task=${epic} .pending=${pending.get().has(epic.id)} .allowed=${can(this.rig, 'plan')} .reason=${whyNot(this.rig, 'plan')} @stop-epic=${this.onStop}></epic-card>
        <div class="layout">
          <section aria-labelledby="tasks-h">
            <h2 id="tasks-h">Tasks</h2>
            ${epic.metadata.factory.children.length === 0 ? html`<p class="empty">No tasks yet — the planner is still working, or the epic is empty.</p>` : html`
            <div class="surface"><table>
              <thead><tr><th>Task</th><th>State</th><th class="num">Attempts</th><th class="num">Tokens</th><th>Branch</th><th>Working</th></tr></thead>
              <tbody>${repeat(epic.metadata.factory.children, (c: Child) => c.id, (c: Child) => html`<tr>
                <td><strong>${c.title}</strong><br><span class="mono muted">${c.id}</span></td>
                <td><span class="badge ${stateTone(c.state)}">${c.state.replace('_', ' ')}</span></td>
                <td class="num">${c.attempts}/${c.attemptLimit}</td>
                <td class="num">${Math.round(c.tokens / 1000)}k</td>
                <td class="mono">${c.branch ?? ''}</td>
                <td class="muted">${c.state === 'in_progress' ? (working.get(c.id)?.title ?? 'session starting') : ''}</td>
              </tr>`)}</tbody>
            </table></div>`}
            ${inbox.length === 0 ? nothing : html`<h2 style="margin-block-start: var(--space-6)">Needs you</h2>
              <div style="display:grid; gap: var(--space-4)">${repeat(inbox, (t) => t.id, (t) => html`<inbox-item .task=${t} .pending=${pending.get().has(t.id)} .allowed=${can(this.rig, 'resolve')} .reason=${whyNot(this.rig, 'resolve')} @apply-option=${this.onOption}></inbox-item>`)}</div>`}
          </section>
          <section aria-labelledby="tl-h">
            <h2 id="tl-h">Timeline</h2>
            ${events.length === 0 ? html`<p class="empty">No events for this epic in the current session.</p>` : html`<ol class="timeline">${repeat(events, (f) => `${String(f.cursor)}:${String(f.record.at)}:${f.record.kind}`, (f) => {
              const line = describe(f) ?? { title: `${f.record.actor}: ${f.record.kind}`, tone: 'info' as const };
              const at = typeof f.record.at === 'number' ? new Date(f.record.at * 1000) : new Date(f.record.at);
              return html`<li class=${line.tone}><span>${line.title}<time datetime=${at.toISOString()}>${at.toLocaleTimeString()}</time></span></li>`;
            })}</ol>`}
          </section>
        </div>`}`;
  }

  private readonly onStop = async (e: CustomEvent<{ id: string }>): Promise<void> => { await stopEpic(this.rig as RigName, e.detail.id); };
  private readonly onOption = async (e: CustomEvent<{ id: string; option: AttentionOption; note: string }>): Promise<void> => {
    await applyOption(this.rig as RigName, e.detail.id, e.detail.option, e.detail.note);
  };
}

declare global {
  interface HTMLElementTagNameMap { 'epic-page': EpicPage; }
}
