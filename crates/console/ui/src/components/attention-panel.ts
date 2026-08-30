import { LitElement, html, css, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import type { Attention, AttentionOption } from '../core/schema.js';
import { surface, controls, badges } from '../styles/shared.js';

/** Evidence and one-click options for something that needs a human. */
@customElement('attention-panel')
export class AttentionPanel extends LitElement {
  static override styles = [surface, controls, badges, css`
    :host { display: block; }
    .panel { display: grid; gap: var(--space-3); }
    .reason { display: grid; gap: .25rem; }
    .reason strong { font-size: 1.05rem; }
    .reason p { color: var(--fg-muted); }
    .facts { display: flex; gap: var(--space-2); flex-wrap: wrap; }
    details { border: 1px solid var(--line); border-radius: var(--radius-sm); }
    summary { cursor: pointer; padding: .5rem .75rem; font-weight: 600; }
    pre { margin: 0; padding: .75rem; white-space: pre-wrap; font-family: var(--mono); font-size: .8rem; max-block-size: 16rem; overflow: auto; border-block-start: 1px solid var(--line); }
    ul { margin: 0; padding-inline-start: 1.2rem; color: var(--fg-muted); }
    .options { display: grid; gap: var(--space-2); }
    .option { display: grid; grid-template-columns: 1fr auto; gap: .25rem .75rem; align-items: center; padding: .6rem .75rem; border: 1px solid var(--line); border-radius: var(--radius-sm); }
    .option.chosen { border-color: var(--accent); box-shadow: var(--glow); }
    .option small { grid-column: 1; color: var(--fg-muted); }
    .note { grid-column: 1 / -1; display: grid; gap: .4rem; }
    .why { font-size: .85rem; color: var(--fg-muted); }
  `];

  @property({ attribute: false }) attention!: Attention;
  @property({ type: Boolean }) pending = false;
  @property({ type: Boolean }) allowed = true;
  @property() reason = '';
  @state() private chosen: AttentionOption | null = null;
  @state() private note = '';

  override render() {
    const a = this.attention;
    return html`<div class="panel">
      <div class="reason">
        <strong>${a.reason.summary}</strong>
        <p>${a.reason.detail}</p>
      </div>
      <div class="facts">
        ${a.attempts ? html`<span class="badge ${a.attempts.used >= a.attempts.limit ? 'danger' : 'info'}">attempts ${a.attempts.used}/${a.attempts.limit}</span>` : nothing}
        ${a.tokens ? html`<span class="badge info">${Math.round(a.tokens.used / 1000)}k / ${Math.round(a.tokens.limit / 1000)}k tokens</span>` : nothing}
        ${a.branch ? html`<span class="badge">${a.branch}</span>` : nothing}
        ${a.taskId ? html`<span class="badge mono">${a.taskId}</span>` : nothing}
      </div>
      ${a.lastVerify ? html`<details><summary>Last verification output</summary><pre>${a.lastVerify}</pre></details>` : nothing}
      ${a.guidance.length > 0 ? html`<div><strong>Guidance already given</strong><ul>${a.guidance.map((g) => html`<li>${g}</li>`)}</ul></div>` : nothing}
      ${this.allowed ? nothing : html`<p class="why" role="note">${this.reason}</p>`}
      <div class="options" role="group" aria-label="What to do">
        ${a.options.map((o) => html`<div class="option ${this.chosen === o.id ? 'chosen' : ''}">
          <span><strong>${o.label}</strong></span>
          <button type="button" class=${o.destructive ? 'danger' : 'primary'} ?disabled=${!this.allowed || this.pending}
            @click=${() => { this.pick(o.id, o.needsNote); }}>${this.pending && this.chosen === o.id ? 'Working…' : this.chosen === o.id && o.needsNote ? 'Confirm' : o.label}</button>
          <small>${o.description}</small>
          ${this.chosen === o.id && o.needsNote ? html`<form class="note" @submit=${this.confirm}>
            <label>Your note<input name="note" required minlength="2" .value=${this.note} @input=${(e: Event) => { this.note = (e.target as HTMLInputElement).value; }} ?disabled=${this.pending}></label>
            <span class="why">The next worker session reads this before it starts.</span>
          </form>` : nothing}
        </div>`)}
      </div>
    </div>`;
  }

  private pick(option: AttentionOption, needsNote: boolean): void {
    if (this.chosen === option && needsNote) { this.emit(); return; }
    this.chosen = option;
    if (!needsNote) this.emit();
  }

  private readonly confirm = (e: Event): void => { e.preventDefault(); this.emit(); };

  private emit(): void {
    if (this.chosen === null) return;
    const spec = this.attention.options.find((o) => o.id === this.chosen);
    if (spec?.needsNote === true && this.note.trim().length < 2) return;
    this.dispatchEvent(new CustomEvent('apply-option', { detail: { id: this.attention.id, option: this.chosen, note: this.note.trim() }, bubbles: true, composed: true }));
  }
}

declare global {
  interface HTMLElementTagNameMap { 'attention-panel': AttentionPanel; }
}
