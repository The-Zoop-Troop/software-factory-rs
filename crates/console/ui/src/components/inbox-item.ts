import { LitElement, html, css, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { attentionOf, messageText, type Task } from '../core/schema.js';
import { surface, controls } from '../styles/shared.js';
import './state-badge.js';
import './attention-panel.js';

@customElement('inbox-item')
export class InboxItem extends LitElement {
  static override styles = [surface, controls, css`
    article { display: grid; gap: var(--space-3); border-inline-start: 4px solid var(--warn); }
    header { display: flex; justify-content: space-between; gap: var(--space-2); flex-wrap: wrap; }
    pre { white-space: pre-wrap; font-family: var(--mono); font-size: .85rem; margin: 0; padding: var(--space-3); border-radius: var(--radius-sm); background: light-dark(oklch(96% 0.01 var(--hue)), oklch(12% 0.02 var(--hue))); max-block-size: 14rem; overflow: auto; }
    form { display: grid; gap: var(--space-2); }
    .why { font-size: .85rem; color: var(--fg-muted); }
  `];

  @property({ attribute: false }) task!: Task;
  @property({ type: Boolean }) pending = false;
  @property({ type: Boolean }) allowed = true;
  @property() reason = '';
  @state() private note = '';

  override render() {
    const f = this.task.metadata.factory;
    const attention = attentionOf(this.task.status.message);
    return html`<article class="surface" aria-labelledby="i-${this.task.id}">
      <header>
        <div><strong id="i-${this.task.id}">${f.kind}</strong> <span class="mono muted">${this.task.id}</span></div>
        <state-badge .state=${this.task.status.state}></state-badge>
      </header>
      ${attention
        ? html`<attention-panel .attention=${attention} ?pending=${this.pending} ?allowed=${this.allowed} .reason=${this.reason}></attention-panel>`
        : html`<pre>${messageText(this.task.status.message)}</pre>
          ${this.allowed ? nothing : html`<p class="why" role="note">${this.reason}</p>`}
          <form @submit=${this.submit}>
            <label>Your answer or resolution
              <input name="note" required minlength="2" .value=${this.note} @input=${(e: Event) => { this.note = (e.target as HTMLInputElement).value; }} ?disabled=${this.pending || !this.allowed}>
            </label>
            <button type="submit" class="primary" ?disabled=${this.pending || !this.allowed || this.note.trim().length < 2}>${this.pending ? 'Resolving…' : 'Resolve'}</button>
          </form>`}
    </article>`;
  }

  private readonly submit = (e: Event): void => {
    e.preventDefault();
    const note = this.note.trim();
    if (note.length < 2) return;
    this.dispatchEvent(new CustomEvent('resolve-item', { detail: { id: this.task.id, note }, bubbles: true, composed: true }));
  };
}

declare global {
  interface HTMLElementTagNameMap { 'inbox-item': InboxItem; }
}
