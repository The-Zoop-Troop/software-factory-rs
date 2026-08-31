import { describe, it, expect, beforeEach } from 'vitest';
import { html } from 'lit';
import { fixture, oneEvent } from '@open-wc/testing-helpers';
import { Schema } from 'effect';
import { RigName, Task } from '../core/schema.js';
import { connectFake, disconnect } from '../core/runtime.js';
import { resetDetail } from '../state/detail.js';
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

const rig = Schema.decodeSync(RigName)('toy');
const settle = () => new Promise((r) => setTimeout(r, 30));
/** Poll until `probe` returns a value; lazy modules and fetch roundtrips need a beat. */
const until = async <T>(probe: () => T | null | undefined, ms = 3000): Promise<T> => {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = probe();
    if (v !== null && v !== undefined) return v;
    if (Date.now() > deadline) throw new Error('timed out waiting');
    await settle();
  }
};

beforeEach(() => { disconnect(); resetDetail(); });

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

describe('task-drawer', () => {
  const bead = {
    id: 'ep-1.2', kind: 'task', title: 'Wire the API', status: 'open', parent: 'ep-1',
    description: 'Add the passthrough route.', acceptance: 'GET /x returns 200.',
    task: {
      state: 'leased', base: 'abc123def456', branch: 'task/ep-1.2', landed: null,
      lease: { holder: 'worker-1', expires: Math.floor(Date.now() / 1000) + 600 },
      budget: { tokens: 1_000_000, attempts: 3, wall_clock_seconds: 3600 },
      usage: { tokens: 250_000, attempts: 1, wall_clock_seconds: 900 },
    },
    verify: { commands: ['npm test', 'npm run lint'], timeout_seconds: 900 },
    notes: [
      { kind: 'verify_block', passed: false, commands: [{ command: 'npm test', status: 'exit 1', tail: '2 failing' }] },
      { kind: 'guidance', text: 'Mock the clock in that test.' },
      { kind: 'plain', text: 'claimed by worker-1' },
    ],
    needs: ['backend/be-1'],
  };

  it('shows meta, meters, verify commands, and the notes biography', async () => {
    await import('./task-drawer.js');
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [] }, beads: { 'toy/ep-1.2': bead as never } }, 'ok');
    const el = await fixture<HTMLElement>(html`<task-drawer rig="toy" .taskId=${'ep-1.2'}></task-drawer>`);
    const root = el.shadowRoot as ShadowRoot;
    await until(() => root.querySelector('dl.meta'));
    expect(root.querySelector('h2')?.textContent).toBe('Wire the API');
    const dds = [...root.querySelectorAll('dl.meta dd')].map((n) => n.textContent);
    expect(dds.some((t) => t.includes('task/ep-1.2'))).toBe(true);
    expect(dds.some((t) => t.includes('worker-1'))).toBe(true);
    expect(dds.some((t) => t.includes('backend/be-1'))).toBe(true);
    const meterLabels = [...root.querySelectorAll('.meter .lbl')].map((n) => n.textContent);
    expect(meterLabels.some((t) => t.includes('250k / 1.0M'))).toBe(true);
    expect([...root.querySelectorAll('.cmds li')].map((n) => n.textContent)).toEqual(['$ npm test', '$ npm run lint']);
    const fail = root.querySelector('details.note.fail');
    expect(fail?.hasAttribute('open')).toBe(true);
    expect(fail?.querySelector('pre')?.textContent).toContain('[exit 1]');
    expect(root.querySelector('.note-line.guidance')?.textContent).toContain('Mock the clock');
    expect(root.textContent).toContain('Add the passthrough route.');
    expect(root.textContent).toContain('GET /x returns 200.');
  });

  it('refetches its bead when a task_update touches it', async () => {
    await import('./task-drawer.js');
    const { touchTask } = await import('../state/detail.js');
    const world = { token: 'ok', rigs: [rig], tasks: { toy: [] }, beads: { 'toy/ep-1.2': bead as never } };
    connectFake(world, 'ok');
    const el = await fixture<HTMLElement>(html`<task-drawer rig="toy" .taskId=${'ep-1.2'}></task-drawer>`);
    const root = el.shadowRoot as ShadowRoot;
    await until(() => root.querySelector('dl.meta'));
    world.beads['toy/ep-1.2'] = { ...bead, title: 'Wire the API v2' } as never;
    touchTask('toy', 'ep-1.2');
    await until(() => (root.querySelector('h2')?.textContent === 'Wire the API v2' ? true : null));
  });
});

describe('request-card expansion', () => {
  it('expands to the full plan text with contract sections', async () => {
    await import('./request-card.js');
    const request = Schema.decodeSync(Task)({ id: 'pr-1', contextId: 'pr-1', status: { state: 'TASK_STATE_SUBMITTED', timestamp: 't' }, metadata: { factory: { kind: 'plan_request', title: 'Portal after backend' } } });
    const bead = {
      id: 'pr-1', kind: 'plan_request', title: 'Portal after backend', status: 'open', parent: null,
      description: 'Build the portal.\n\n## Upstream contracts (landed; build on these)\n\n### backend/be-1\nrange abc..def\n',
      acceptance: null, task: null, verify: null, notes: [], needs: ['backend/be-1'],
    };
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [request] }, beads: { 'toy/pr-1': bead as never } }, 'ok');
    const el = await fixture<HTMLElement>(html`<request-card .task=${request} rig="toy"></request-card>`);
    const root = el.shadowRoot as ShadowRoot;
    (root.querySelector('button.expand') as HTMLButtonElement).click();
    const plan = await until(() => root.querySelector('.plan pre'));
    expect(plan.textContent).toBe('Build the portal.');
    expect(root.querySelector('details.contract summary')?.textContent).toContain('backend/be-1');
  });
});

describe('help-tip and help-drawer', () => {
  it('help-tip toggles an anchored popover with the explainer text', async () => {
    await import('./help-tip.js');
    const el = await fixture<HTMLElement>(html`<help-tip text="Explains a widget." label="About widgets"></help-tip>`);
    const root = el.shadowRoot as ShadowRoot;
    const btn = root.querySelector('button') as HTMLButtonElement;
    expect(btn.getAttribute('aria-label')).toBe('About widgets');
    const pop = root.querySelector('[popover]') as HTMLElement;
    expect(pop.textContent).toContain('Explains a widget.');
    btn.click();
    await settle();
    expect(pop.matches(':popover-open')).toBe(true);
    btn.click();
    await settle();
    expect(pop.matches(':popover-open')).toBe(false);
  });

  it('help-drawer opens a glossary dialog with every term', async () => {
    await import('./help-drawer.js');
    const { GLOSSARY } = await import('../copy.js');
    const el = await fixture<HTMLElement>(html`<help-drawer></help-drawer>`);
    const root = el.shadowRoot as ShadowRoot;
    expect(root.querySelector('button')?.getAttribute('aria-label')).toBe('Help and glossary');
    (root.querySelector('button') as HTMLButtonElement).click();
    await (el as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    expect((root.querySelector('dialog') as HTMLDialogElement).open).toBe(true);
    expect(root.querySelectorAll('dt').length).toBe(GLOSSARY.length);
    expect(root.textContent).toContain('Epic');
    (root.querySelectorAll('button')[1] as HTMLButtonElement).click();
    await (el as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    expect((root.querySelector('dialog') as HTMLDialogElement).open).toBe(false);
  });
});
