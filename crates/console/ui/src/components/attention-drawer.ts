import { LitElement, html, css, nothing } from 'lit';
import { customElement, query } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { repeat } from 'lit/directives/repeat.js';
import { attentionOf } from '../core/schema.js';
import { attentionItems } from '../state/rigs.js';
import { controls, badges } from '../styles/shared.js';
import { EMPTY, SECTION_HELP } from '../copy.js';

/** The badge in the header and the drawer it opens: every item that needs a human, all rigs. */
@customElement('attention-drawer')
export class AttentionDrawer extends SignalWatcher(LitElement) {
  static override styles = [controls, badges, css`
    :host { display: inline-block; }
    .badge-button { border: 0; background: transparent; padding: 0; cursor: pointer; }
    .badge-button:disabled { cursor: default; }
    dialog { inset-inline: auto 0; inset-block: 0; margin: 0; block-size: 100dvh; max-block-size: 100dvh; inline-size: min(28rem, 92vw);
             border: 0; border-inline-start: 1px solid var(--line); background: var(--bg-elev); backdrop-filter: blur(16px); color: var(--fg); padding: var(--space-6);
             transition: translate 250ms var(--ease-out), overlay 250ms allow-discrete, display 250ms allow-discrete; }
    dialog[open] { translate: 0 0; @starting-style { translate: 100% 0; } }
    dialog:not([open]) { translate: 100% 0; }
    dialog::backdrop { background: oklch(0% 0 0 / 0.3); }
    header { display: flex; justify-content: space-between; align-items: center; margin-block-end: var(--space-4); }
    h2 { font-size: 1.2rem; font-weight: 800; }
    ol { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--space-3); }
    li { display: grid; gap: .25rem; padding: var(--space-3); border: 1px solid var(--line); border-radius: var(--radius); border-inline-start: 4px solid var(--warn); }
    li a { font-weight: 700; color: inherit; text-decoration: none; }
    li a:hover { text-decoration: underline; }
    .meta { font-size: .85rem; color: var(--fg-muted); display: flex; gap: .5rem; flex-wrap: wrap; }
    .empty { color: var(--fg-muted); }
    .hint { color: var(--fg-muted); font-size: .85rem; margin-block-end: var(--space-3); text-wrap: pretty; }
  `];

  @query('dialog') private dialog?: HTMLDialogElement;

  override render() {
    const items = attentionItems.get();
    const n = items.length;
    return html`
      <button type="button" class="badge-button" aria-label="${n} item${n === 1 ? '' : 's'} need you" ?disabled=${n === 0} @click=${() => this.dialog?.showModal()}>
        <span class="badge ${n > 0 ? 'warn' : ''}" role="status">${n} need${n === 1 ? 's' : ''} you</span>
      </button>
      <dialog aria-labelledby="att-h" @click=${this.onBackdrop}>
        <header><h2 id="att-h">Needs you</h2><button type="button" @click=${() => this.dialog?.close()}>Close</button></header>
        <p class="hint">${SECTION_HELP.attention}</p>
        ${n === 0 ? html`<p class="empty">${EMPTY.attention}</p>` : html`<ol>
          ${repeat(items, (i) => `${i.rig}/${i.task.id}`, (i) => {
            const a = attentionOf(i.task.status.message);
            const epic = a?.epicId ?? i.task.contextId;
            return html`<li>
              <a href="/rigs/${i.rig}/epics/${epic}" @click=${() => this.dialog?.close()}>${a?.reason.summary ?? i.task.metadata.factory.title}</a>
              <span class="meta"><span class="mono">${i.rig}</span><span class="mono">${i.task.id}</span>${a?.taskId ? html`<span class="mono">${a.taskId}</span>` : nothing}</span>
              <span class="meta">${a?.reason.detail ?? ''}</span>
            </li>`;
          })}
        </ol>`}
      </dialog>`;
  }

  private readonly onBackdrop = (e: MouseEvent): void => {
    if (e.target === this.dialog) this.dialog.close();
  };
}

declare global {
  interface HTMLElementTagNameMap { 'attention-drawer': AttentionDrawer; }
}
