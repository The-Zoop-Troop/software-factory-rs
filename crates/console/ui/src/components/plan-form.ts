import { LitElement, html, css } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { surface, controls } from '../styles/shared.js';

/** A plan is prose; anything bigger than this is not a plan file. */
const maxPlanFileBytes = 512 * 1024;

type FileNote = { readonly kind: 'loaded' | 'error'; readonly text: string };

@customElement('plan-form')
export class PlanForm extends LitElement {
  static override styles = [surface, controls, css`
    form { display: grid; gap: var(--space-3); }
    textarea { min-block-size: 5.5rem; field-sizing: content; resize: vertical; }
    textarea.dropping { outline: 2px dashed var(--accent); outline-offset: -2px; }
    .row { display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
    .actions { display: inline-flex; gap: var(--space-2); align-items: center; }
    .hint { font-size: .85rem; color: var(--fg-muted); }
    .hint.error { color: var(--danger); }
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
  @state() private fileNote: FileNote | null = null;
  @state() private dropping = false;
  /** `rig/epic` choices the plan may wait for (other rigs' open epics). */
  @property({ attribute: false }) choices: ReadonlyArray<{ readonly rig: string; readonly epic: string; readonly title: string }> = [];

  override render() {
    const busy = this.pending || !this.allowed;
    const hint = !this.allowed
      ? this.reason
      : (this.fileNote?.text ?? "The rig's planner turns this into an epic of verified tasks.");
    return html`<form class="surface" @submit=${this.submit}>
      <label>Plan — what should the factory build?
        <textarea name="plan" required minlength="8" class=${this.dropping ? 'dropping' : ''} placeholder="Add a reverse function to lib.sh with a test and a README entry." .value=${this.text} @input=${this.onInput} @dragover=${this.onDragOver} @dragleave=${this.onDragLeave} @drop=${this.onDrop} ?disabled=${busy}></textarea>
      </label>
      ${this.choices.length === 0 ? '' : html`<label class="after">After (the plan waits, then sees their contracts)
        <select multiple name="after" size="3" ?disabled=${busy} @change=${(e: Event) => { this.waitsFor = Array.from((e.target as HTMLSelectElement).selectedOptions, (o) => o.value); }}>
          ${this.choices.map((c) => html`<option value="${c.rig}/${c.epic}">${c.rig} / ${c.epic} — ${c.title}</option>`)}
        </select>
      </label>`}
      <div class="row">
        <span class=${this.allowed && this.fileNote?.kind === 'error' ? 'hint error' : 'hint'}>${hint}</span>
        <span class="actions">
          <input type="file" hidden accept=".md,.markdown,.txt,text/plain,text/markdown" @change=${this.onFileChosen}>
          ${this.pending
            ? html`<span class="pending" aria-live="polite"><span class="spinner" aria-hidden="true"></span>${this.pendingText}</span>`
            : html`<button type="button" ?disabled=${busy} @click=${this.pickFile}>Load file…</button>
              <button type="submit" class="primary" ?disabled=${!this.allowed || this.text.trim().length < 8}>Plan</button>`}
        </span>
      </div>
    </form>`;
  }

  private readonly onInput = (e: Event): void => {
    this.text = (e.target as HTMLTextAreaElement).value;
    this.fileNote = null;
  };

  private readonly pickFile = (): void => {
    this.renderRoot.querySelector<HTMLInputElement>('input[type=file]')?.click();
  };

  private readonly onFileChosen = (e: Event): void => {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (file !== undefined) void this.loadFile(file);
  };

  private readonly onDragOver = (e: DragEvent): void => {
    if (this.pending || !this.allowed) return;
    e.preventDefault();
    this.dropping = true;
  };

  private readonly onDragLeave = (): void => {
    this.dropping = false;
  };

  private readonly onDrop = (e: DragEvent): void => {
    e.preventDefault();
    this.dropping = false;
    if (this.pending || !this.allowed) return;
    const file = e.dataTransfer?.files[0];
    if (file !== undefined) void this.loadFile(file);
  };

  /** Read a plan file in the browser and fill the textarea for review; nothing is uploaded. */
  private async loadFile(file: File): Promise<void> {
    if (file.size > maxPlanFileBytes) {
      this.fileNote = { kind: 'error', text: `${file.name} is ${String(Math.round(file.size / 1024))} KiB — a plan is prose (limit 512 KiB).` };
      return;
    }
    const content = await file.text().catch(() => null);
    if (content === null || content.includes('\u0000')) {
      this.fileNote = { kind: 'error', text: `Could not read ${file.name} as text.` };
      return;
    }
    if (content.trim().length === 0) {
      this.fileNote = { kind: 'error', text: `${file.name} is empty.` };
      return;
    }
    this.text = content;
    this.fileNote = { kind: 'loaded', text: `Loaded ${file.name} — review, then Plan.` };
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
    this.fileNote = null;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'plan-form': PlanForm; }
}
