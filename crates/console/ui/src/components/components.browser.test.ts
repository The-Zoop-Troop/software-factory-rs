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
    (root.querySelector('footer button') as HTMLButtonElement).click();
    await el.updateComplete;
    expect((root.querySelector('dialog') as HTMLDialogElement).open).toBe(true);
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
    expect(root.textContent).toContain('Queuing');
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

describe('attention-panel and scope-aware controls', () => {
  const attention = {
    kind: 'incident', id: 'inc-1', taskId: 'ep-1.3', epicId: 'ep-1',
    reason: { kind: 'merge_conflict', summary: 'The branch no longer merges', detail: 'lib.sh' },
    attempts: { used: 3, limit: 3 }, tokens: { used: 12000, limit: 400000 }, branch: 'task/x',
    lastVerify: 'verify FAILED\n$ sh t.sh\n[exit 1]', guidance: ['use sh'],
    options: [
      { id: 'retry_fresh' as const, label: 'Retry', description: 'd', needsNote: false, destructive: false },
      { id: 'retry_with_guidance' as const, label: 'Retry with guidance', description: 'd', needsNote: true, destructive: false },
      { id: 'stop_epic' as const, label: 'Stop the epic', description: 'd', needsNote: false, destructive: true },
    ],
  };

  it('shows evidence and emits options, with notes when required', async () => {
    const { AttentionPanel } = await import('./attention-panel.js');
    void AttentionPanel;
    const el = await fixture<HTMLElement & { attention: typeof attention }>(html`<attention-panel .attention=${attention}></attention-panel>`);
    const root = el.shadowRoot as ShadowRoot;
    expect(root.textContent).toContain('attempts 3/3');
    expect(root.querySelector('pre')?.textContent).toContain('exit 1');
    expect(root.textContent).toContain('use sh');
    const buttons = [...root.querySelectorAll('.option button')] as HTMLButtonElement[];
    setTimeout(() => { buttons[0]?.click(); });
    const ev = await oneEvent(el, 'apply-option');
    expect((ev as CustomEvent<{ option: string }>).detail.option).toBe('retry_fresh');
    buttons[1]?.click();
    await (el as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    const input = root.querySelector('.note input') as HTMLInputElement;
    input.value = 'try POSIX';
    input.dispatchEvent(new Event('input'));
    setTimeout(() => { (root.querySelector('.note') as HTMLFormElement).requestSubmit(); });
    const ev2 = await oneEvent(el, 'apply-option');
    expect((ev2 as CustomEvent<{ option: string; note: string }>).detail).toMatchObject({ option: 'retry_with_guidance', note: 'try POSIX' });
  });

  it('disables controls without scope and explains why', async () => {
    const el = await fixture<EpicCard>(html`<epic-card .task=${epic} .allowed=${false} reason="no plan scope"></epic-card>`);
    const root = el.shadowRoot as ShadowRoot;
    expect((root.querySelector('footer button') as HTMLButtonElement).disabled).toBe(true);
    expect(root.textContent).toContain('no plan scope');
    const form = await fixture<PlanForm>(html`<plan-form .allowed=${false} reason="watch only"></plan-form>`);
    expect((form.shadowRoot as ShadowRoot).textContent).toContain('watch only');
    const item = await fixture<InboxItem>(html`<inbox-item .task=${incident} .allowed=${false} reason="no resolve"></inbox-item>`);
    expect((item.shadowRoot as ShadowRoot).textContent).toContain('no resolve');
  });

  it('asks for confirmation before stopping an epic', async () => {
    const el = await fixture<EpicCard>(html`<epic-card .task=${epic}></epic-card>`);
    const root = el.shadowRoot as ShadowRoot;
    (root.querySelector('footer button') as HTMLButtonElement).click();
    await el.updateComplete;
    const dialog = root.querySelector('dialog') as HTMLDialogElement;
    expect(dialog.open).toBe(true);
    setTimeout(() => { (root.querySelector('dialog form') as HTMLFormElement).requestSubmit(); });
    const ev = await oneEvent(el, 'stop-epic');
    expect((ev as CustomEvent<{ id: string }>).detail.id).toBe('ep-1');
  });
});

describe('rig-facts', () => {
  it('renders the posture badge, fact rows, and totals strip', async () => {
    await import('./rig-facts.js');
    const { Schema } = await import('effect');
    const { RigDetail } = await import('../core/schema.js');
    const detail = Schema.decodeUnknownSync(RigDetail)({
      rig: 'toy',
      facts: { repo_url: 'https://github.com/x/y.git', runtime: 'node', harness: 'claude', main: 'feat/z' },
      posture: 'stopped',
      ledger_ms: null,
      events: { count: 7, last_at: null },
      budget: { max_tokens: 5_000_000, max_usd_micros: null },
      rollup: { epics: 2, tasks_landed: 6, tasks_planned: 6, first_pass: 5, tokens: 42_000, work_seconds: 3600, retry_tax_seconds: 0 },
    });
    const el = await fixture<HTMLElement>(html`<rig-facts .detail=${detail}></rig-facts>`);
    const root = el.shadowRoot as ShadowRoot;
    expect(root.querySelector('.badge')?.textContent).toContain('stopped');
    const labels = [...root.querySelectorAll('dt')].map((n) => n.textContent);
    expect(labels).toEqual(['Repo', 'Branch', 'Runtime', 'Harness', 'Budget']);
    expect(root.querySelector('dd a')?.getAttribute('href')).toBe('https://github.com/x/y.git');
    const tots = [...root.querySelectorAll('.tot .lbl')].map((n) => n.textContent);
    expect(tots).toEqual(['epics', 'tasks landed', 'first pass', 'tokens', 'work', 'events']);
    expect([...root.querySelectorAll('.tot .num')].map((n) => n.textContent)).toContain('42k');
  });

  it('renders nothing at all without a detail', async () => {
    await import('./rig-facts.js');
    const el = await fixture<HTMLElement>(html`<rig-facts></rig-facts>`);
    expect((el.shadowRoot as ShadowRoot).textContent.trim()).toBe('');
  });
});
