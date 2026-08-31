// A small "?" affordance that opens a short, anchored explainer. Popover API +
// CSS anchor positioning: keyboard and touch friendly, no library. Browsers
// without anchor positioning center the popover, which still reads fine.
import { LitElement, html, css } from 'lit';
import { customElement, property } from 'lit/decorators.js';

@customElement('help-tip')
export class HelpTip extends LitElement {
  static override styles = css`
    :host { display: inline-flex; vertical-align: middle; }
    button {
      inline-size: 1.1rem; block-size: 1.1rem; padding: 0; border-radius: 50%;
      border: 1px solid var(--line); background: transparent; color: var(--fg-muted);
      font-size: 0.7rem; font-weight: 700; line-height: 1; font-family: var(--font);
      display: inline-grid; place-content: center; cursor: help;
      anchor-name: --tip; touch-action: manipulation;
      transition: color 150ms var(--ease-out), border-color 150ms var(--ease-out);
    }
    button:hover, button:focus-visible, button:has(+ :popover-open) { color: var(--accent); border-color: var(--accent); }
    [popover] {
      position: fixed;
      position-anchor: --tip;
      position-area: block-start;
      position-try-fallbacks: flip-block, flip-inline;
      margin: 0.4rem; padding: 0.6rem 0.8rem; max-inline-size: min(22rem, 88vw);
      border: 1px solid var(--line); border-radius: var(--radius-sm);
      background: var(--bg-elev); color: var(--fg); backdrop-filter: blur(12px);
      box-shadow: var(--shadow); font-family: var(--font); font-size: 0.85rem; font-weight: 400;
      line-height: 1.45; letter-spacing: normal; text-transform: none; white-space: normal; text-wrap: pretty;
    }
  `;

  /** The explainer sentence(s). */
  @property() text = '';
  /** Accessible name for the "?" button. */
  @property() label = 'What is this?';

  override render() {
    return html`<button type="button" popovertarget="tip" aria-label=${this.label}>?</button>
      <div id="tip" popover>${this.text}</div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'help-tip': HelpTip; }
}
