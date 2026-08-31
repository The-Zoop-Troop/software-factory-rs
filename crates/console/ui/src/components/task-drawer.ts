// A slide-in drawer with everything the factory knows about one task bead:
// meta (branch, base, landed, lease), budget-vs-used meters, verify commands,
// the attempt strip, and the structured notes biography.
import { LitElement, html, css, nothing, type PropertyValues } from 'lit';
import { customElement, property, query } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { loadBeadDetail } from '../actions.js';
import type { BeadDetail, NoteSegment, RigName } from '../core/schema.js';
import { fmtDuration, fmtTokens } from '../state/detail.js';
import { beadDetails, taskTouched } from '../state/detail.js';
import { STAGE_LABEL, mmss, type Row } from '../state/gantt.js';
import { badges, controls, surface } from '../styles/shared.js';
import { SECTION_HELP } from '../copy.js';
import { lockScroll, unlockScroll } from '../core/scroll-lock.js';
import './help-tip.js';

interface Meter {
  readonly label: string;
  readonly used: string;
  readonly pct: number;
}

/** Budget-vs-used rows; a task without budget shows nothing rather than zeros. */
export const meters = (t: NonNullable<BeadDetail['task']>): ReadonlyArray<Meter> => {
  const rows: Meter[] = [];
  const add = (label: string, used: number, limit: number, show: string): void => {
    if (limit > 0) rows.push({ label, used: show, pct: Math.min(100, Math.round((used / limit) * 100)) });
  };
  add('tokens', t.usage.tokens, t.budget.tokens, `${fmtTokens(t.usage.tokens)} / ${fmtTokens(t.budget.tokens)}`);
  add('attempts', t.usage.attempts, t.budget.attempts, `${String(t.usage.attempts)} / ${String(t.budget.attempts)}`);
  add('wall clock', t.usage.wall_clock_seconds, t.budget.wall_clock_seconds, `${fmtDuration(t.usage.wall_clock_seconds)} / ${fmtDuration(t.budget.wall_clock_seconds)}`);
  return rows;
};

const segmentText = (seg: NoteSegment): string => (seg.kind === 'verify_block' ? '' : seg.text);

/** "expires in 8m" / "expired" — how long a worker's claim still holds. */
export const leaseLeft = (nowMs: number, expiresUnix: number): string => {
  const left = expiresUnix - Math.floor(nowMs / 1000);
  return left <= 0 ? 'expired' : `expires in ${fmtDuration(left)}`;
};

@customElement('task-drawer')
export class TaskDrawer extends SignalWatcher(LitElement) {
  static override styles = [surface, controls, badges, css`
    :host { display: contents; }
    dialog {
      inset-inline: auto 0; inset-block: 0; margin: 0; padding: 0;
      inline-size: min(36rem, 94vw); block-size: 100dvh; max-block-size: 100dvh; max-inline-size: none;
      border: 0; border-inline-start: 1px solid var(--line);
      background: var(--bg); color: var(--fg); box-shadow: var(--shadow-raised);
      transition: translate 480ms var(--ease-spring), overlay 480ms allow-discrete, display 480ms allow-discrete;
    }
    dialog[open] { translate: 0 0; @starting-style { translate: 100% 0; } }
    dialog:not([open]) { translate: 100% 0; transition-duration: 220ms; transition-timing-function: var(--ease-out); }
    dialog::backdrop {
      background: oklch(0% 0 0 / 0.35); backdrop-filter: blur(3px);
      transition: background-color 480ms var(--ease-out), backdrop-filter 480ms var(--ease-out), overlay 480ms allow-discrete, display 480ms allow-discrete;
    }
    dialog[open]::backdrop { @starting-style { background: oklch(0% 0 0 / 0); backdrop-filter: blur(0); } }
    .panel {
      block-size: 100%; overflow: auto; overscroll-behavior: contain; padding: var(--space-5);
      display: grid; gap: var(--space-4); align-content: start;
      scrollbar-width: thin; scrollbar-color: color-mix(in oklch, var(--fg-muted) 35%, transparent) transparent;
    }
    /* Sections rise in a beat behind the sheet. */
    .panel > * { animation: rise 500ms var(--ease-out) backwards; }
    .panel > :nth-child(2) { animation-delay: 60ms; }
    .panel > :nth-child(3) { animation-delay: 100ms; }
    .panel > :nth-child(4) { animation-delay: 130ms; }
    .panel > :nth-child(n + 5) { animation-delay: 150ms; }
    @keyframes rise { from { opacity: 0; translate: 0 12px; } }
    header { display: flex; align-items: start; gap: var(--space-3); }
    header .titles { display: grid; gap: 2px; flex: 1; min-inline-size: 0; }
    h2 { font-size: 1.1rem; font-weight: 800; margin: 0; }
    h3 { font-size: 0.78rem; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--fg-muted); margin: 0 0 var(--space-2); display: flex; gap: var(--space-2); align-items: center; }
    /* Every block in the sheet is a quiet card — same language as the pages. */
    .panel > section, .panel > dl.meta { border: 1px solid var(--line); border-radius: var(--radius); padding: var(--space-3) var(--space-4); margin: 0; }
    .close { border: none; background: none; font-size: 1.2rem; cursor: pointer; color: var(--fg-muted); inline-size: 2.25rem; block-size: 2.25rem; display: grid; place-content: center; border-radius: 50%; touch-action: manipulation; }
    dl.meta { display: grid; grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); gap: var(--space-2) var(--space-3); margin: 0; }
    dl.meta dt { font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--fg-muted); }
    dl.meta dd { margin: 0; overflow-wrap: anywhere; }
    .meter { display: grid; gap: 2px; }
    .meter .bar { block-size: 6px; border-radius: 999px; background: color-mix(in oklch, var(--fg-muted) 12%, transparent); overflow: hidden; }
    .meter .fill { block-size: 100%; border-radius: inherit; background: var(--accent); }
    .meter .fill.hot { background: var(--danger); }
    .meter .lbl { display: flex; justify-content: space-between; font-size: 0.8rem; color: var(--fg-muted); }
    ul.cmds { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--space-1); }
    .lane { position: relative; height: 1.4rem; background: color-mix(in oklch, var(--fg-muted) 8%, transparent); border-radius: 4px; }
    .seg { position: absolute; top: 0; bottom: 0; min-width: 2px; border-radius: 3px; opacity: 0.95; }
    .seg.retry { opacity: 0.45; }
    .seg.queue_wait { background: oklch(80% 0.02 260); }
    .seg.session { background: oklch(65% 0.18 260); }
    .seg.verify_wait { background: oklch(82% 0.04 150); }
    .seg.verify { background: oklch(65% 0.17 150); }
    .seg.integrate_wait { background: oklch(84% 0.05 60); }
    .seg.integrate { background: oklch(68% 0.16 60); }
    details.note { border: 1px solid var(--line); border-radius: var(--radius-sm); padding: var(--space-2) var(--space-3); }
    details.note summary { cursor: pointer; font-weight: 700; }
    details.note.pass summary { color: light-dark(oklch(from var(--ok) 38% c h), oklch(from var(--ok) 85% c h)); }
    details.note.fail summary { color: var(--danger); }
    details.note pre { margin: var(--space-1) 0 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 0.8rem; color: var(--fg-muted); max-block-size: 14rem; overflow: auto; }
    .note-line { border-inline-start: 3px solid var(--line); padding-inline-start: var(--space-3); white-space: pre-wrap; overflow-wrap: anywhere; }
    .note-line.guidance { border-color: var(--warn); }
    .note-line.operator { border-color: var(--info, var(--accent)); }
    p.prose { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  `];

  @property() rig = '';
  @property({ attribute: false }) taskId = '';
  /** The task's row in the epic's gantt layout, if metrics are loaded. */
  @property({ attribute: false }) row: Row | null = null;
  @property({ attribute: false }) span = 1;

  private seenTouch = 0;
  @query('dialog') private dialog?: HTMLDialogElement;

  private locked = false;

  override firstUpdated(): void {
    this.dialog?.showModal();
    lockScroll();
    this.locked = true;
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this.locked) { unlockScroll(); this.locked = false; }
  }

  override updated(changed: PropertyValues): void {
    if (changed.has('taskId') && this.taskId !== '') void loadBeadDetail(this.rig as RigName, this.taskId);
  }

  private key(): string { return `${this.rig}/${this.taskId}`; }

  override render() {
    // A pushed task_update for this bead means the biography moved: refetch it.
    const touched = taskTouched.get()[this.key()] ?? 0;
    if (touched !== this.seenTouch) {
      this.seenTouch = touched;
      if (touched !== 0) void loadBeadDetail(this.rig as RigName, this.taskId);
    }
    const d = beadDetails.get()[this.key()];
    return html`<dialog aria-label="Task detail" @click=${this.onBackdrop} @close=${this.onClosed}>
      <div class="panel">
      <header>
        <div class="titles">
          <h2>${d?.title ?? this.taskId}</h2>
          <span class="mono muted">${this.taskId}${d?.task === null || d === undefined ? '' : ` · ${d.task.state}`}</span>
        </div>
        <button class="close" aria-label="Close" @click=${this.onClose}>✕</button>
      </header>
      ${d === undefined ? html`<p class="muted">Loading…</p>` : html`
        ${this.renderMeta(d)}
        ${d.task === null ? nothing : this.renderMeters(d.task)}
        ${this.renderAttempts()}
        ${this.renderVerify(d)}
        ${d.description === '' ? nothing : html`<section><h3>Description</h3><p class="prose">${d.description}</p></section>`}
        ${d.acceptance === null ? nothing : html`<section><h3>Acceptance</h3><p class="prose">${d.acceptance}</p></section>`}
        ${this.renderNotes(d)}`}
      </div>
    </dialog>`;
  }

  private renderMeta(d: BeadDetail) {
    const t = d.task;
    return html`<dl class="meta">
      ${t === null ? nothing : html`<div><dt>Branch</dt><dd class="mono">${t.branch}</dd></div>
        <div><dt>Base</dt><dd class="mono">${t.base}</dd></div>
        ${t.landed === null ? nothing : html`<div><dt>Landed</dt><dd class="mono">${t.landed.slice(0, 12)}</dd></div>`}
        ${t.lease === null ? nothing : html`<div><dt>Leased by</dt><dd>${t.lease.holder} <span class="muted">(${leaseLeft(Date.now(), t.lease.expires)})</span></dd></div>`}`}
      ${d.needs === null || d.needs.length === 0 ? nothing : html`<div><dt>Needs</dt><dd class="mono">${d.needs.join(', ')}</dd></div>`}
    </dl>`;
  }

  private renderMeters(t: NonNullable<BeadDetail['task']>) {
    const rows = meters(t);
    return rows.length === 0 ? nothing : html`<section><h3>Budget <help-tip text=${SECTION_HELP.budget} label="About budgets"></help-tip></h3>
      <div style="display:grid; gap: var(--space-2)">${rows.map((m) => html`<div class="meter">
        <span class="lbl"><span>${m.label}</span><span>${m.used}</span></span>
        <span class="bar"><span class="fill ${m.pct >= 90 ? 'hot' : ''}" style="inline-size:${String(m.pct)}%"></span></span>
      </div>`)}</div></section>`;
  }

  private renderAttempts() {
    const r = this.row;
    if (r === null || r.segments.length === 0) return nothing;
    const pct = (v: number): string => `${String((v / this.span) * 100)}%`;
    return html`<section><h3>Attempts</h3>
      <div class="lane">${r.segments.map((s) => html`<div class="seg ${s.stage} ${s.landed ? '' : 'retry'}" style="left:${pct(s.start)};width:${pct(Math.max(s.end - s.start, this.span / 200))}" title="${STAGE_LABEL[s.stage]} · attempt ${s.attempt + 1} · ${mmss(s.end - s.start)}"></div>`)}</div>
    </section>`;
  }

  private renderVerify(d: BeadDetail) {
    const v = d.verify;
    return v === null || v.commands.length === 0 ? nothing : html`<section><h3>Verify</h3>
      <ul class="cmds">${v.commands.map((c) => html`<li class="mono">$ ${c}</li>`)}</ul>
      <p class="muted" style="margin: var(--space-1) 0 0">timeout ${fmtDuration(v.timeout_seconds)}</p></section>`;
  }

  private renderNotes(d: BeadDetail) {
    if (d.notes.length === 0) return nothing;
    return html`<section><h3>Notes</h3><div style="display:grid; gap: var(--space-2)">
      ${d.notes.map((seg) => seg.kind === 'verify_block'
        ? html`<details class="note ${seg.passed ? 'pass' : 'fail'}" ?open=${!seg.passed}>
            <summary>verify ${seg.passed ? 'PASSED' : 'FAILED'} · ${String(seg.commands.length)} command${seg.commands.length === 1 ? '' : 's'}</summary>
            ${seg.commands.map((c) => html`<pre>$ ${c.command}  [${c.status}]${c.tail === '' ? '' : `\n${c.tail}`}</pre>`)}
          </details>`
        : seg.kind === 'guidance' || seg.kind === 'operator'
          ? html`<div class="note-line ${seg.kind}"><strong>${seg.kind}:</strong> ${segmentText(seg)}</div>`
          : html`<p class="prose muted">${segmentText(seg)}</p>`)}
    </div></section>`;
  }

  private readonly onClose = (): void => {
    this.dialog?.close();
  };

  private readonly onBackdrop = (e: MouseEvent): void => {
    if (e.target === this.dialog) this.dialog.close();
  };

  private readonly onClosed = (): void => {
    if (this.locked) { unlockScroll(); this.locked = false; }
    this.dispatchEvent(new CustomEvent('close'));
  };
}

declare global {
  interface HTMLElementTagNameMap { 'task-drawer': TaskDrawer }
}
