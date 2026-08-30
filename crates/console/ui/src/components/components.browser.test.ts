import { describe, it, expect } from 'vitest';
import { html } from 'lit';
import { fixture, oneEvent } from '@open-wc/testing-helpers';
import { Schema } from 'effect';
import { Task } from '../core/schema.js';
import { notify, reset as resetNotices } from '../state/notices.js';
import { lastError } from '../state/session.js';
import './epic-card.js';
import './plan-form.js';
import './inbox-item.js';
import './toast-stack.js';
import './error-panel.js';
import './state-badge.js';
import type { EpicCard } from './epic-card.js';
import type { PlanForm } from './plan-form.js';
import type { InboxItem } from './inbox-item.js';
import type { ToastStack } from './toast-stack.js';
import type { ErrorPanel } from './error-panel.js';

const epic = Schema.decodeSync(Task)({ id: 'ep-1', contextId: 'ep-1', status: { state: 'TASK_STATE_WORKING', timestamp: 't' }, metadata: { factory: { kind: 'epic', title: 'Build it', tasks: 4, closed: 1, incidents: 1 } } });
const done = { ...epic, status: { ...epic.status, state: 'TASK_STATE_COMPLETED' as const } };
const incident = Schema.decodeSync(Task)({ id: 'inc-1', contextId: 'ep-1', status: { state: 'TASK_STATE_INPUT_REQUIRED', timestamp: 't', message: { messageId: 'm', role: 'ROLE_AGENT', parts: [{ text: 'why?' }] } }, metadata: { factory: { kind: 'incident' } } });

describe('epic-card', () => {
  it('shows progress and emits stop', async () => {
    const el = await fixture<EpicCard>(html`<epic-card .task=${epic}></epic-card>`);
    const root = el.shadowRoot as ShadowRoot;
    expect(root.querySelector('h3')?.textContent).toContain('Build it');
    expect(root.querySelector('output')?.textContent).toContain('1/4');
    expect(root.querySelector('[role=progressbar]')?.getAttribute('aria-valuenow')).toBe('25');
    setTimeout(() => root.querySelector('button')?.click());
    const ev = await oneEvent(el, 'stop-epic');
    expect((ev as CustomEvent<{ id: string }>).detail.id).toBe('ep-1');
    const finished = await fixture<EpicCard>(html`<epic-card .task=${done}></epic-card>`);
    expect((finished.shadowRoot as ShadowRoot).querySelector('button')).toBeNull();
  });
});

describe('plan-form', () => {
  it('disables until text is long enough, emits, clears, shows pending', async () => {
    const el = await fixture<PlanForm>(html`<plan-form></plan-form>`);
    const root = el.shadowRoot as ShadowRoot;
    const ta = root.querySelector('textarea') as HTMLTextAreaElement;
    expect((root.querySelector('button[type=submit]') as HTMLButtonElement).disabled).toBe(true);
    ta.value = 'build a thing please';
    ta.dispatchEvent(new Event('input'));
    await el.updateComplete;
    expect((root.querySelector('button[type=submit]') as HTMLButtonElement).disabled).toBe(false);
    setTimeout(() => root.querySelector('form')?.requestSubmit());
    const ev = await oneEvent(el, 'submit-plan');
    expect((ev as CustomEvent<{ text: string }>).detail.text).toBe('build a thing please');
    el.clear();
    el.pending = true;
    await el.updateComplete;
    expect(root.textContent).toContain('Planning');
  });
});

describe('inbox-item', () => {
  it('shows the question and emits resolve with the note', async () => {
    const el = await fixture<InboxItem>(html`<inbox-item .task=${incident}></inbox-item>`);
    const root = el.shadowRoot as ShadowRoot;
    expect(root.querySelector('pre')?.textContent).toBe('why?');
    const input = root.querySelector('input') as HTMLInputElement;
    input.value = 'fixed';
    input.dispatchEvent(new Event('input'));
    await el.updateComplete;
    setTimeout(() => root.querySelector('form')?.requestSubmit());
    const ev = await oneEvent(el, 'resolve-item');
    expect((ev as CustomEvent<{ id: string; note: string }>).detail).toEqual({ id: 'inc-1', note: 'fixed' });
  });
});

describe('toast-stack and error-panel', () => {
  it('render notices and the last error', async () => {
    resetNotices();
    const toasts = await fixture<ToastStack>(html`<toast-stack></toast-stack>`);
    notify('success', 'Landed', 'ep-1');
    await toasts.updateComplete;
    expect((toasts.shadowRoot as ShadowRoot).textContent).toContain('Landed');
    (toasts.shadowRoot as ShadowRoot).querySelector('button')?.click();
    await toasts.updateComplete;
    expect((toasts.shadowRoot as ShadowRoot).textContent).not.toContain('Landed');
    const panel = await fixture<ErrorPanel>(html`<error-panel></error-panel>`);
    lastError.set({ title: 'Not allowed', detail: 'd', recovery: 'r' });
    await panel.updateComplete;
    expect((panel.shadowRoot as ShadowRoot).querySelector('[role=alert]')?.textContent).toContain('Not allowed');
    lastError.set(null);
  });
});
