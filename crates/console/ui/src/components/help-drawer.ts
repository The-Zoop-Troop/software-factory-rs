// The header's "?" button: a one-screen orientation to the console plus a
// glossary of factory vocabulary, in a slide-in dialog.
import { LitElement, html, css } from 'lit';
import { customElement, query } from 'lit/decorators.js';
import { GLOSSARY } from '../copy.js';
import { controls } from '../styles/shared.js';

@customElement('help-drawer')
export class HelpDrawer extends LitElement {
  static override styles = [controls, css`
    :host { display: inline-block; }
    dialog { inset-inline: auto 0; inset-block: 0; margin: 0; block-size: 100dvh; max-block-size: 100dvh; inline-size: min(30rem, 94vw);
             border: 0; border-inline-start: 1px solid var(--line); background: var(--bg-elev); backdrop-filter: blur(16px); color: var(--fg); padding: var(--space-6);
             overflow-y: auto; overscroll-behavior: contain;
             transition: translate 250ms var(--ease-out), overlay 250ms allow-discrete, display 250ms allow-discrete; }
    dialog[open] { translate: 0 0; @starting-style { translate: 100% 0; } }
    dialog:not([open]) { translate: 100% 0; }
    dialog::backdrop { background: oklch(0% 0 0 / 0.3); }
    header { display: flex; justify-content: space-between; align-items: center; margin-block-end: var(--space-3); }
    h2 { font-size: 1.2rem; font-weight: 800; margin: 0; }
    .what { color: var(--fg-muted); margin-block-end: var(--space-4); text-wrap: pretty; }
    dl { margin: 0; display: grid; gap: var(--space-3); }
    dt { font-weight: 700; }
    dd { margin: 0; color: var(--fg-muted); font-size: 0.92rem; text-wrap: pretty; }
  `];

  @query('dialog') private dialog?: HTMLDialogElement;

  override render() {
    return html`
      <button type="button" aria-label="Help and glossary" @click=${() => this.dialog?.showModal()}>?</button>
      <dialog aria-labelledby="help-h" @click=${this.onBackdrop}>
        <header><h2 id="help-h">How to read this console</h2><button type="button" @click=${() => this.dialog?.close()}>Close</button></header>
        <p class="what">This console is the operator’s window into the factory: submit plans, watch rigs break them into epics of verified tasks, and step in when something needs a human.</p>
        <dl>${GLOSSARY.map((g) => html`<div><dt>${g.term}</dt><dd>${g.def}</dd></div>`)}</dl>
      </dialog>`;
  }

  private readonly onBackdrop = (e: MouseEvent): void => {
    if (e.target === this.dialog) this.dialog.close();
  };
}

declare global {
  interface HTMLElementTagNameMap { 'help-drawer': HelpDrawer; }
}
