import { LitElement, html, css } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { surface, controls } from '../styles/shared.js';

@customElement('plan-form')
export class PlanForm extends LitElement {
  static override styles = [surface, controls, css`
    form { display: grid; gap: var(--space-3); }
    textarea { min-block-size: 5.5rem; field-sizing: content; resize: vertical; }
    .row { display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
    .hint { font-size: .85rem; color: var(--fg-muted); }
    .pending { display: inline-flex; gap: .5rem; align-items: center; color: var(--accent-strong); font-weight: 600; }
    .spinner { inline-size: 1em; block-size: 1em; border-radius: 50%; border: 2px solid var(--accent-soft); border-top-color: var(--accent); animation: spin 800ms linear infinite; }
    @keyframes spin { to { transform: rotate(360deg); } }
  `];

  @property({ type: Boolean }) pending = false;
  @property({ type: Boolean }) allowed = true;
  @property() reason = '';
  @property() pendingText = 'Queuing…';
  @state() private text = '';
  @state() private waitsFor: ReadonlyArray<string> = [];
  /** `rig/epic` choices the plan may wait for (other rigs' open epics). */
  @property({ attribute: false }) choices: ReadonlyArray<{ readonly rig: string; readonly epic: string; readonly title: string }> = [];

  override render() {
    return html`<form class="surface" @submit=${this.submit}>
      <label>Plan — what should the factory build?
        <textarea name="plan" required minlength="8" placeholder="Add a reverse function to lib.sh with a test and a README entry." .value=${this.text} @input=${(e: Event) => { this.text = (e.target as HTMLTextAreaElement).value; }} ?disabled=${this.pending || !this.allowed}></textarea>
      </label>
      ${this.choices.length === 0 ? '' : html`<label class="after">After (the plan waits, then sees their contracts)
        <select multiple name="after" size="3" ?disabled=${this.pending || !this.allowed} @change=${(e: Event) => { this.waitsFor = Array.from((e.target as HTMLSelectElement).selectedOptions, (o) => o.value); }}>
          ${this.choices.map((c) => html`<option value="${c.rig}/${c.epic}">${c.rig} / ${c.epic} — ${c.title}</option>`)}
        </select>
      </label>`}
      <div class="row">
        <span class="hint">${this.allowed ? "The rig's planner turns this into an epic of verified tasks." : this.reason}</span>
        ${this.pending
          ? html`<span class="pending" aria-live="polite"><span class="spinner" aria-hidden="true"></span>${this.pendingText}</span>`
          : html`<button type="submit" class="primary" ?disabled=${!this.allowed || this.text.trim().length < 8}>Plan</button>`}
      </div>
    </form>`;
  }

  private readonly submit = (e: Event): void => {
    e.preventDefault();
    const text = this.text.trim();
    if (text.length < 8) return;
    const needs = this.waitsFor.map((v) => { const [rig = '', epic = ''] = v.split('/'); return { rig, epic }; });
    this.dispatchEvent(new CustomEvent('submit-plan', { detail: { text, needs }, bubbles: true, composed: true }));
  };

  clear(): void {
    this.text = '';
    this.waitsFor = [];
  }
}

declare global {
  interface HTMLElementTagNameMap { 'plan-form': PlanForm; }
}
