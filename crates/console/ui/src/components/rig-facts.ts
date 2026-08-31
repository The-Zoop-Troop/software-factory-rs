// The rig page's header panel: host facts, posture, and lifetime totals.
import { LitElement, html, css, nothing } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import type { RigDetail } from '../core/schema.js';
import { factRows, postureLabel, totals } from '../state/detail.js';
import { badges, surface } from '../styles/shared.js';

@customElement('rig-facts')
export class RigFactsCard extends LitElement {
  static override styles = [surface, badges, css`
    .facts { display: grid; gap: var(--space-3); }
    dl { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr)); gap: var(--space-2) var(--space-4); margin: 0; }
    dl div { display: grid; gap: 2px; min-inline-size: 0; }
    dt { font-size: 0.72rem; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--fg-muted); }
    dd { margin: 0; overflow-wrap: anywhere; }
    dd a { color: var(--accent-strong); }
    .mono { font-family: var(--mono); font-size: 0.85em; }
    .totals { display: flex; flex-wrap: wrap; gap: var(--space-4); border-block-start: 1px solid var(--line); padding-block-start: var(--space-3); }
    .tot { display: grid; gap: 1px; }
    .tot .num { font-weight: 800; font-variant-numeric: tabular-nums; }
    .tot .lbl { font-size: 0.72rem; color: var(--fg-muted); text-transform: uppercase; letter-spacing: 0.06em; }
  `];

  @property({ attribute: false }) detail: RigDetail | null = null;

  override render() {
    const d = this.detail;
    if (d === null) return nothing;
    const p = postureLabel(d.posture);
    return html`<section class="surface facts" aria-label="Rig facts">
      <span class="badge ${p.tone}">${p.label}</span>
      <dl>${factRows(d, Date.now()).map((r) => html`<div>
        <dt>${r.label}</dt>
        <dd class=${r.mono === true ? 'mono' : ''}>${r.href === undefined ? r.value : html`<a href=${r.href} target="_blank" rel="noreferrer">${r.value}</a>`}</dd>
      </div>`)}</dl>
      <div class="totals">${totals(d).map((t) => html`<div class="tot"><span class="num">${t.value}</span><span class="lbl">${t.label}</span></div>`)}</div>
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'rig-facts': RigFactsCard }
}
