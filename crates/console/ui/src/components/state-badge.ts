import { LitElement, html, css } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import type { TaskState } from '../core/schema.js';
import { badges } from '../styles/shared.js';

export const stateLabel = (s: TaskState): { readonly text: string; readonly tone: string } => {
  switch (s) {
    case 'TASK_STATE_SUBMITTED': return { text: 'queued', tone: 'info' };
    case 'TASK_STATE_WORKING': return { text: 'working', tone: 'working' };
    case 'TASK_STATE_INPUT_REQUIRED': return { text: 'needs you', tone: 'warn' };
    case 'TASK_STATE_COMPLETED': return { text: 'done', tone: 'ok' };
    case 'TASK_STATE_FAILED': return { text: 'failed', tone: 'danger' };
    case 'TASK_STATE_CANCELED': return { text: 'canceled', tone: 'danger' };
    case 'TASK_STATE_REJECTED': return { text: 'rejected', tone: 'danger' };
  }
};

@customElement('state-badge')
export class StateBadge extends LitElement {
  static override styles = [badges, css`:host { display: inline-block; }`];

  @property() state: TaskState = 'TASK_STATE_SUBMITTED';

  override render() {
    const { text, tone } = stateLabel(this.state);
    return html`<span class="badge ${tone}">${text}</span>`;
  }
}

declare global {
  interface HTMLElementTagNameMap { 'state-badge': StateBadge; }
}
