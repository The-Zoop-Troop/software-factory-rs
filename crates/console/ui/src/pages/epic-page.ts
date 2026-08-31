import { LitElement, html, css, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { applyOption, loadBeadDetail, loadEpicConsumers, loadEpicMetrics, pending, refreshRig, stopEpic, loadEpicHistory } from '../actions.js';
import type { AttentionOption, Child, RigName } from '../core/schema.js';
import { attentionOf } from '../core/schema.js';
import { PAGE, SECTION_HELP } from '../copy.js';
import { describe, forEpic, latestProgress, recordDate } from '../state/events.js';
import { currentRig, taskById, tasksByRig } from '../state/rigs.js';
import { beadDetails, consumersByEpic, fmtTokens, metricsByEpic } from '../state/detail.js';
import { layout, mmss } from '../state/gantt.js';
import { can, whyNot } from '../state/session.js';
import { badges, controls, surface } from '../styles/shared.js';
import '../components/epic-card.js';
import '../components/help-tip.js';
import '../components/inbox-item.js';
import '../components/task-drawer.js';

const stateTone = (s: string): string =>
  s === 'closed' ? 'ok' : s === 'incident' ? 'danger' : s === 'open' ? 'info' : 'working';

@customElement('epic-page')
export class EpicPage extends SignalWatcher(LitElement) {
  static override styles = [surface, controls, badges, css`
    :host { display: grid; gap: var(--space-6); }
    .intro { display: grid; gap: var(--space-2); }
    header { display: flex; align-items: baseline; gap: var(--space-3); flex-wrap: wrap; }
    header a { color: inherit; }
    h1 { font-size: 1.5rem; font-weight: 800; font-family: var(--mono); overflow-wrap: anywhere; }
    h2 { font-size: 1.1rem; font-weight: 700; color: var(--fg-muted); margin-block-end: var(--space-3); display: flex; gap: var(--space-2); align-items: center; }
    .layout { display: grid; gap: var(--space-6); grid-template-columns: minmax(0, 2fr) minmax(0, 1fr); }
    @media (max-width: 60rem) { .layout { grid-template-columns: 1fr; } }
    .surface:has(> table) { overflow-x: auto; }
    table { inline-size: 100%; border-collapse: collapse; font-size: .92rem; }
    th, td { text-align: start; padding: .5rem .6rem; border-block-end: 1px solid var(--line); vertical-align: top; }
    th { color: var(--fg-muted); font-weight: 600; font-size: .8rem; text-transform: uppercase; letter-spacing: .04em; }
    td.num { font-variant-numeric: tabular-nums; text-align: end; }
    /* The event list stays compact: it scrolls inside its own box instead of stretching the page. */
    .timeline { list-style: none; margin: 0; padding: 0; display: grid; gap: .5rem; position: relative;
      max-block-size: 28rem; overflow-y: auto; overscroll-behavior: contain;
      scrollbar-width: thin; scrollbar-color: color-mix(in oklch, var(--fg-muted) 35%, transparent) transparent;
      padding-inline-end: var(--space-2); }
    .timeline::before { content: ''; position: absolute; inset-block: .4rem; inset-inline-start: .45rem; inline-size: 2px; background: var(--line); }
    .timeline li { display: grid; grid-template-columns: 1rem 1fr; gap: .6rem; align-items: baseline; }
    .timeline li::before { content: ''; inline-size: .6rem; block-size: .6rem; border-radius: 50%; background: var(--tone, var(--fg-muted)); border: 2px solid var(--bg); box-shadow: 0 0 0 2px var(--tone, var(--line)); z-index: 1; }
    .success { --tone: var(--ok); } .warning { --tone: var(--warn); } .danger { --tone: var(--danger); } .info { --tone: var(--info); }
    .timeline time { color: var(--fg-muted); font-family: var(--mono); font-size: .75rem; margin-inline-start: .5rem; }
    /* Metrics as a stat-tile strip: big value over a small label, on its own card. */
    .rollup-card { display: grid; gap: var(--space-3); }
    .rollup { display: flex; flex-wrap: wrap; gap: var(--space-3) var(--space-8); align-items: start; }
    .rollup .stat { display: grid; gap: 2px; }
    .rollup .stat b { font-size: 1.25rem; font-weight: 650; line-height: 1.1; color: var(--fg); }
    .rollup .stat span { font-size: .72rem; font-weight: 600; letter-spacing: .05em; text-transform: uppercase; color: var(--fg-muted); }
    .rollup help-tip { margin-inline-start: auto; align-self: start; }
    .stamps { color: var(--fg-muted); font-size: .8rem; font-family: var(--mono); margin: 0; border-block-start: 1px solid var(--line); padding-block-start: var(--space-3); }
    .subhead { font-size: .72rem; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--fg-muted); margin: 0 0 var(--space-2); }
    details.plan { border: 1px solid var(--line); border-radius: var(--radius-sm); padding: var(--space-2) var(--space-3); }
    details.plan summary { cursor: pointer; font-weight: 700; }
    details.plan pre { margin: var(--space-2) 0 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: .85rem; }
    ul.consumers { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--space-2); }
    ul.consumers li { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2) var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--line); border-radius: var(--radius-sm); }
    ul.consumers a.rig { font-weight: 700; font-family: var(--mono); color: var(--accent-strong); text-decoration: none; }
    ul.consumers a.rig:hover { text-decoration: underline; }
    ul.consumers .title { flex: 1 1 12rem; min-inline-size: 0; }
    tbody tr { cursor: pointer; }
    @media (hover: hover) { tbody tr:hover { background: color-mix(in oklch, var(--accent) 6%, transparent); } }
    .tasklink { border: none; background: none; padding: 0; font: inherit; color: inherit; cursor: pointer; text-align: start; }
  `];

  @property() rig = '';
  @property() id = '';
  @state() private selected: string | null = null;

  override connectedCallback(): void {
    super.connectedCallback();
    currentRig.set(this.rig as RigName);
    if (taskById(this.rig, this.id) === undefined) void refreshRig(this.rig as RigName);
    void loadEpicHistory(this.rig as RigName, this.id);
    void loadBeadDetail(this.rig as RigName, this.id);
    void loadEpicMetrics(this.rig as RigName, this.id);
    void loadEpicConsumers(this.rig as RigName, this.id);
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
    const events = all.slice(-80).reverse();
    return html`
      <div class="intro">
        <header><a href="/">Rigs</a><span class="muted">/</span><a href="/rigs/${this.rig}">${this.rig}</a><span class="muted">/</span><h1>${this.id}</h1><a class="throughput" href="/rigs/${this.rig}/epics/${this.id}/throughput">throughput →</a></header>
        <p class="page-desc">${PAGE.epic.desc}</p>
      </div>
      ${epic === undefined ? html`<p class="empty">Loading ${this.id}…</p>` : html`
        <epic-card .task=${epic} .pending=${pending.get().has(epic.id)} .allowed=${can(this.rig, 'plan')} .reason=${whyNot(this.rig, 'plan')} @stop-epic=${this.onStop}></epic-card>
        ${this.renderRollup(all)}
        <div class="layout">
          <section aria-labelledby="tasks-h">
            <h2 id="tasks-h">Tasks <help-tip text=${SECTION_HELP.tasks} label="About tasks"></help-tip></h2>
            ${epic.metadata.factory.children.length === 0 ? html`<p class="empty">No tasks yet — the planner is still working, or the epic is empty.</p>` : html`
            <div class="surface"><table>
              <thead><tr><th>Task</th><th>State</th><th class="num">Attempts</th><th class="num">Tokens</th><th>Branch</th><th>Working</th></tr></thead>
              <tbody>${repeat(epic.metadata.factory.children, (c: Child) => c.id, (c: Child) => html`<tr @click=${() => { this.openTask(c.id); }}>
                <td><button class="tasklink" @click=${(e: Event) => { e.stopPropagation(); this.openTask(c.id); }}><strong>${c.title}</strong></button><br><span class="mono muted">${c.id}</span></td>
                <td><span class="badge ${stateTone(c.state)}">${c.state.replace('_', ' ')}</span></td>
                <td class="num">${c.attempts}/${c.attemptLimit}</td>
                <td class="num">${Math.round(c.tokens / 1000)}k</td>
                <td class="mono">${c.branch ?? ''}</td>
                <td class="muted">${c.state === 'in_progress' ? (working.get(c.id)?.title ?? 'session starting') : ''}</td>
              </tr>`)}</tbody>
            </table></div>`}
            ${inbox.length === 0 ? nothing : html`<h2 style="margin-block-start: var(--space-6)">Needs you <help-tip text=${SECTION_HELP.needsYou} label="About this section"></help-tip></h2>
              <div style="display:grid; gap: var(--space-4)">${repeat(inbox, (t) => t.id, (t) => html`<inbox-item .task=${t} .pending=${pending.get().has(t.id)} .allowed=${can(this.rig, 'resolve')} .reason=${whyNot(this.rig, 'resolve')} @apply-option=${this.onOption}></inbox-item>`)}</div>`}
            ${this.renderPlan()}
            ${this.renderProvenance()}
          </section>
          <section aria-labelledby="tl-h">
            <h2 id="tl-h">Timeline <help-tip text=${SECTION_HELP.timeline} label="About the timeline"></help-tip></h2>
            ${events.length === 0 ? html`<p class="empty">No events for this epic in the current session.</p>` : html`<div class="surface"><ol class="timeline">${repeat(events, (f) => `${String(f.cursor)}:${String(f.record.at)}:${f.record.kind}`, (f) => {
              const line = describe(f) ?? { title: `${f.record.actor}: ${f.record.kind}`, tone: 'info' as const };
              const at = recordDate(f.record.at);
              return html`<li class=${line.tone}><span>${line.title}<time datetime=${at.toISOString()}>${at.toLocaleTimeString()}</time></span></li>`;
            })}</ol></div>`}
          </section>
        </div>
        ${this.selected === null ? nothing : this.renderDrawer(this.selected)}`}`;
  }

  /** Wall-clock, work, parallelism, first-pass, retry tax, tokens — plus lifecycle stamps. */
  private renderRollup(all: ReadonlyArray<{ readonly record: { readonly kind: string; readonly at: string | number } }>) {
    const m = metricsByEpic.get()[`${this.rig}/${this.id}`];
    const planned = all.find((f) => f.record.kind === 'task_planned');
    const closed = all.find((f) => f.record.kind === 'epic_closed');
    const stamp = (label: string, at: string | number | undefined): string =>
      at === undefined ? '' : `${label} ${recordDate(at).toLocaleString()}`;
    const stamps = [stamp('planned', planned?.record.at), stamp('closed', closed?.record.at)].filter((x) => x !== '').join(' · ');
    if (m === undefined && stamps === '') return nothing;
    return html`<section class="surface rollup-card" aria-label="Epic metrics">
      ${m === undefined ? nothing : html`<div class="rollup">
        <div class="stat"><b>${mmss(m.wall_clock)}</b><span>wall-clock</span></div>
        <div class="stat"><b>${mmss(m.work)}</b><span>work</span></div>
        <div class="stat"><b>${String(m.parallelism_pct)}%</b><span>parallelism</span></div>
        <div class="stat"><b>${String(m.first_pass)}/${String(m.landed)}</b><span>first pass</span></div>
        <div class="stat"><b>${mmss(m.retry_tax)}</b><span>retry tax</span></div>
        <div class="stat"><b>${fmtTokens(m.tokens)}</b><span>tokens</span></div>
        <help-tip text=${SECTION_HELP.rollup} label="About these metrics"></help-tip>
      </div>`}
      ${stamps === '' ? nothing : html`<p class="stamps">${stamps}</p>`}
    </section>`;
  }

  /** The plan triptych: what was asked (description), what to know (references), what landed. */
  private renderPlan() {
    const d = beadDetails.get()[`${this.rig}/${this.id}`];
    if (d === undefined) return nothing;
    const refs = (d.context ?? []).filter((c) => c.kind === 'reference');
    const contracts = (d.context ?? []).filter((c) => c.kind === 'contract');
    if (d.description === '' && refs.length === 0 && contracts.length === 0) return nothing;
    return html`<h2 style="margin-block-start: var(--space-6)">Plan <help-tip text=${SECTION_HELP.plan} label="About the plan"></help-tip></h2>
      <div class="surface" style="display:grid; gap: var(--space-2)">
        ${d.description === '' ? nothing : html`<details class="plan" open><summary>Plan text</summary><pre>${d.description}</pre></details>`}
        ${refs.map((c) => html`<details class="plan"><summary>Reference — ${c.title}</summary><pre>${c.text}</pre></details>`)}
        ${contracts.map((c) => html`<details class="plan"><summary>Contract — what this epic landed</summary><pre>${c.text}</pre></details>`)}
      </div>`;
  }

  /** Where the epic came from, and who builds on it. */
  private renderProvenance() {
    const d = beadDetails.get()[`${this.rig}/${this.id}`];
    const consumers = consumersByEpic.get()[`${this.rig}/${this.id}`] ?? [];
    const origin = d?.origin ?? null;
    if (origin === null && consumers.length === 0) return nothing;
    return html`<h2 style="margin-block-start: var(--space-6)">Provenance <help-tip text=${SECTION_HELP.provenance} label="About provenance"></help-tip></h2>
      <div class="surface" style="display:grid; gap: var(--space-2)">
        ${origin === null ? nothing : html`<details class="plan"><summary>From plan request — ${origin.title}</summary><pre>${origin.text}</pre></details>`}
        ${consumers.length === 0 ? nothing : html`<div>
          <p class="subhead">Built on by</p>
          <ul class="consumers">${consumers.map((c) => html`<li>
            <a class="rig" href="/rigs/${c.rig}">${c.rig}</a>
            <span class="title">${c.title}</span>
            <span class="badge ${c.status === 'closed' ? 'ok' : 'info'}">${c.status}</span>
            <span class="muted mono">${c.id}</span>
          </li>`)}</ul>
        </div>`}
      </div>`;
  }

  private renderDrawer(id: string) {
    const m = metricsByEpic.get()[`${this.rig}/${this.id}`];
    const l = m === undefined ? null : layout(m);
    const row = l?.rows.find((r) => r.task === id) ?? null;
    return html`<task-drawer .rig=${this.rig} .taskId=${id} .row=${row} .span=${l?.span ?? 1} @close=${() => { this.selected = null; }}></task-drawer>`;
  }

  private openTask(id: string): void {
    this.selected = id;
    void loadEpicMetrics(this.rig as RigName, this.id);
  }

  private readonly onStop = async (e: CustomEvent<{ id: string }>): Promise<void> => { await stopEpic(this.rig as RigName, e.detail.id); };
  private readonly onOption = async (e: CustomEvent<{ id: string; option: AttentionOption; note: string }>): Promise<void> => {
    await applyOption(this.rig as RigName, e.detail.id, e.detail.option, e.detail.note);
  };
}

declare global {
  interface HTMLElementTagNameMap { 'epic-page': EpicPage; }
}
