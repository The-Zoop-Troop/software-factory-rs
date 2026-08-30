import { LitElement, html, css, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { Effect } from 'effect';
import type { EpicMetrics, RigName } from '../core/schema.js';
import { run, withApi } from '../core/runtime.js';
import { explain } from '../core/errors.js';
import { lastError } from '../state/session.js';
import { currentRig } from '../state/rigs.js';
import { layout, mmss, STAGE_LABEL, type Stage } from '../state/gantt.js';
import { controls } from '../styles/shared.js';

const STAGES: ReadonlyArray<Stage> = ['queue_wait', 'session', 'verify_wait', 'verify', 'integrate_wait', 'integrate'];
const isStage = (s: string): s is Stage => (STAGES as ReadonlyArray<string>).includes(s);
const stageLabel = (s: string): string => (isStage(s) ? STAGE_LABEL[s] : s);

/** One epic's throughput: a Gantt of every attempt by stage, the stage table, and the totals. */
@customElement('throughput-page')
export class ThroughputPage extends SignalWatcher(LitElement) {
  static override styles = [controls, css`
    :host { display: grid; gap: var(--space-6); }
    header { display: flex; align-items: baseline; gap: var(--space-3); flex-wrap: wrap; }
    h1 { font-size: 1.6rem; font-weight: 800; font-family: var(--mono); }
    h2 { font-size: 1.1rem; font-weight: 700; color: var(--fg-muted); margin-block-end: var(--space-3); }
    .muted { color: var(--fg-muted); }
    .totals { display: flex; flex-wrap: wrap; gap: var(--space-3) var(--space-6); font-variant-numeric: tabular-nums; }
    .totals b { font-family: var(--mono); }
    .gantt { display: grid; grid-template-columns: minmax(8rem, 14rem) 1fr; gap: var(--space-2) var(--space-3); align-items: center; }
    .gantt .task { font-family: var(--mono); font-size: 0.85em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .lane { position: relative; height: 1.4rem; background: color-mix(in oklch, var(--fg-muted) 8%, transparent); border-radius: var(--radius-s, 4px); }
    .seg { position: absolute; top: 0; bottom: 0; min-width: 2px; border-radius: 3px; opacity: 0.95; }
    .seg.retry { opacity: 0.45; }
    .seg.queue_wait { background: oklch(80% 0.02 260); }
    .seg.session { background: oklch(65% 0.18 260); }
    .seg.verify_wait { background: oklch(82% 0.04 150); }
    .seg.verify { background: oklch(65% 0.17 150); }
    .seg.integrate_wait { background: oklch(84% 0.05 60); }
    .seg.integrate { background: oklch(68% 0.16 60); }
    .legend { display: flex; flex-wrap: wrap; gap: var(--space-3); font-size: 0.85em; }
    .legend span::before { content: ''; display: inline-block; inline-size: 0.9em; block-size: 0.9em; border-radius: 2px; margin-inline-end: 0.35em; vertical-align: -0.1em; }
    .legend .queue_wait::before { background: oklch(80% 0.02 260); }
    .legend .session::before { background: oklch(65% 0.18 260); }
    .legend .verify_wait::before { background: oklch(82% 0.04 150); }
    .legend .verify::before { background: oklch(65% 0.17 150); }
    .legend .integrate_wait::before { background: oklch(84% 0.05 60); }
    .legend .integrate::before { background: oklch(68% 0.16 60); }
    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; padding: var(--space-2) var(--space-3); border-block-end: 1px solid var(--line); }
    .num { text-align: right; font-variant-numeric: tabular-nums; }
    .empty { color: var(--fg-muted); }
  `];

  @property() rig = '';
  @property() id = '';
  @state() private report: EpicMetrics | null | undefined = undefined;

  override connectedCallback(): void {
    super.connectedCallback();
    currentRig.set(this.rig as RigName);
    void this.load();
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    currentRig.set(null);
  }

  private async load(): Promise<void> {
    const r = await run(
      withApi((api) => api.metrics(this.rig as RigName, this.id)).pipe(
        Effect.catchAll((e) => Effect.sync(() => { lastError.set(explain(e)); return null; })),
      ),
    ).catch(() => null);
    this.report = r;
  }

  override render() {
    const m = this.report;
    if (m === undefined) return html`${this.header()}<p class="empty">Reading the event log…</p>`;
    if (m === null || m.tasks.length === 0) return html`${this.header()}<p class="empty">No stage data for ${this.id} in this rig's log.</p>`;
    const l = layout(m);
    const pct = (s: number) => `${String((s / l.span) * 100)}%`;
    const peak = m.concurrency.reduce((p, [, n]) => Math.max(p, n), 0);
    return html`
      ${this.header()}
      <section aria-label="Totals" class="totals">
        <span>wall-clock <b>${mmss(m.wall_clock)}</b></span>
        <span>work <b>${mmss(m.work)}</b></span>
        <span>parallelism <b>${m.parallelism_pct}%</b></span>
        <span>critical path <b>${mmss(m.critical_path)}</b></span>
        <span>retry tax <b>${mmss(m.retry_tax)}</b></span>
        <span>first-pass <b>${m.first_pass}/${m.tasks.length}</b></span>
        <span>peak live sessions <b>${peak}</b></span>
        <span>more workers could save up to <b>${mmss(Math.max(0, m.wall_clock - m.critical_path))}</b></span>
      </section>
      <section aria-labelledby="gantt-h">
        <h2 id="gantt-h">Every attempt, by stage</h2>
        <div class="legend">${STAGES.map((s) => html`<span class=${s}>${STAGE_LABEL[s]}</span>`)}</div>
        <div class="gantt" role="list">
          ${repeat(l.rows, (r) => r.task, (r) => html`
            <div class="task" role="listitem" title=${r.task}>${r.task}</div>
            <div class="lane">${r.segments.map((s) => html`<div class="seg ${s.stage} ${s.landed ? '' : 'retry'}" style="left:${pct(s.start)};width:${pct(Math.max(s.end - s.start, l.span / 400))}" title="${STAGE_LABEL[s.stage]} · attempt ${s.attempt + 1} · ${mmss(s.end - s.start)}"></div>`)}</div>`)}
        </div>
        <p class="muted">${mmss(l.span)} across; faded segments are attempts that did not land.</p>
      </section>
      <section aria-labelledby="stages-h">
        <h2 id="stages-h">Stages</h2>
        <div class="surface"><table>
          <thead><tr><th>Stage</th><th class="num">n</th><th class="num">p50</th><th class="num">max</th><th class="num">total</th></tr></thead>
          <tbody>${repeat(m.stages, (s) => s.stage, (s) => html`<tr><td>${stageLabel(s.stage)}</td><td class="num">${s.samples}</td><td class="num">${mmss(s.p50)}</td><td class="num">${mmss(s.max)}</td><td class="num">${mmss(s.total)}</td></tr>`)}</tbody>
        </table></div>
      </section>
      ${nothing}`;
  }

  private header() {
    return html`<header><a href="/">Rigs</a><span class="muted">/</span><a href="/rigs/${this.rig}">${this.rig}</a><span class="muted">/</span><a href="/rigs/${this.rig}/epics/${this.id}">${this.id}</a><span class="muted">/</span><h1>throughput</h1></header>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'throughput-page': ThroughputPage; }
}
