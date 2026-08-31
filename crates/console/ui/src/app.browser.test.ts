import { describe, it, expect, beforeEach } from 'vitest';
import { html } from 'lit';
import { fixture } from '@open-wc/testing-helpers';
import { Schema } from 'effect';
import { connectFake, disconnect } from './core/runtime.js';
import { RigName, Task } from './core/schema.js';
import { reset as resetRigs } from './state/rigs.js';
import { reset as resetSession, saveToken } from './state/session.js';
import { matchRoute } from './routes.js';
import './app-shell.js';
import './pages/overview-page.js';
import './pages/rig-page.js';
import type { AppShell } from './app-shell.js';
import type { RigPage } from './pages/rig-page.js';

const rig = Schema.decodeSync(RigName)('toy');
const epic = Schema.decodeSync(Task)({ id: 'ep-1', contextId: 'ep-1', status: { state: 'TASK_STATE_WORKING', timestamp: 't' }, metadata: { factory: { kind: 'epic', title: 'Build it', tasks: 2 } } });
const incident = Schema.decodeSync(Task)({ id: 'inc-1', contextId: 'ep-1', status: { state: 'TASK_STATE_INPUT_REQUIRED', timestamp: 't', message: { messageId: 'm', role: 'ROLE_AGENT', parts: [{ text: 'why?' }] } }, metadata: { factory: { kind: 'incident' } } });

const settle = () => new Promise((r) => setTimeout(r, 30));
/** Poll until `probe` returns a value; CI machines load lazy page modules slower than 30 ms. */
const until = async <T>(probe: () => T | null | undefined, ms = 3000): Promise<T> => {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = probe();
    if (v !== null && v !== undefined) return v;
    if (Date.now() > deadline) throw new Error('timed out waiting');
    await settle();
  }
};

beforeEach(() => { resetRigs(); resetSession(); disconnect(); localStorage.clear(); history.replaceState(null, '', '/'); });

describe('routes', () => {
  it('match the overview and a rig', () => {
    expect(matchRoute(new URL('http://x/'))?.route.path).toBe('/');
    expect(matchRoute(new URL('http://x/rigs/toy'))?.params['rig']).toBe('toy');
    expect(matchRoute(new URL('http://x/nope/x'))).toBeNull();
  });
});

describe('throughput-page', () => {
  it('draws a lane per task and the stage table from the metrics endpoint', async () => {
    await import('./pages/throughput-page.js');
    const attempt = { claimed: 10, submitted: 70, verify_started: 80, verified: 90, passed: true, integrate_started: 95, integrated: 100, landed: true, ended_by: null, tokens: 5 };
    const report = { epic: 'ep-1', wall_clock: 100, work: 60, parallelism_pct: 60, critical_path: 60, retry_tax: 0, first_pass: 1, landed: 1, tokens: 5, stages: [{ stage: 'session', samples: 1, p50: 60, max: 60, total: 60 }], concurrency: [[10, 1], [70, 0]] as Array<[number, number]>, tasks: [{ task: 'ep-1.1', planned: 0, needs: [], attempts: [attempt] }] };
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [epic] }, metrics: { 'toy/ep-1': report } }, 'ok');
    const page = await fixture<HTMLElement>(html`<throughput-page rig="toy" id="ep-1"></throughput-page>`);
    const root = page.shadowRoot as ShadowRoot;
    const lanes = await until(() => { const l = root.querySelectorAll('.lane'); return l.length > 0 ? l : null; });
    expect(lanes.length).toBe(1);
    expect(root.querySelectorAll('.seg').length).toBe(6);
    expect(root.querySelector('tbody')?.textContent).toContain('1:00');
    expect(root.querySelector('.totals')?.textContent).toContain('0:40');
  });
});

describe('rig-page history', () => {
  it('lists completed epics under a collapsed section and the closed epic page replays its log', async () => {
    await import('./pages/epic-page.js');
    const done = Schema.decodeSync(Task)({ id: 'ep-0', contextId: 'ep-0', status: { state: 'TASK_STATE_COMPLETED', timestamp: 't' }, metadata: { factory: { kind: 'epic', title: 'Shipped it', tasks: 2, closed: 2, children: [{ id: 'ep-0.1', title: 'a', state: 'closed', attempts: 1, attemptLimit: 3, tokens: 4000, closed: true }] } } });
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [epic] }, history: { toy: [done] }, events: { 'toy/ep-0': [{ at: 1, actor: 'planner', bead: 'ep-0.1', kind: 'task_planned' }, { at: 2, actor: 'worker', bead: 'ep-0.1', kind: 'claimed' }] } }, 'ok');
    const page = await fixture<RigPage>(html`<rig-page rig="toy"></rig-page>`);
    const root = page.shadowRoot as ShadowRoot;
    const details = await until(() => root.querySelector('details.completed'));
    expect(details.querySelector('summary')?.textContent).toContain('Completed');
    expect(details.querySelector('tbody a')?.getAttribute('href')).toBe('/rigs/toy/epics/ep-0');
    expect(details.querySelector('tbody')?.textContent).toContain('2/2');

    const ep = await fixture<HTMLElement>(html`<epic-page rig="toy" id="ep-0"></epic-page>`);
    const er = ep.shadowRoot as ShadowRoot;
    const items = await until(() => { const li = er.querySelectorAll('.timeline li'); return li.length >= 2 ? li : null; });
    expect(items.length).toBe(2);
    expect(er.querySelector('epic-card')).not.toBeNull();
  });
});

describe('rig-page', () => {
  it('renders epics and inbox from the store and drives actions', async () => {
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [epic, incident] } }, 'ok');
    const page = await fixture<RigPage>(html`<rig-page rig="toy"></rig-page>`);
    await settle();
    await page.updateComplete;
    const root = page.shadowRoot as ShadowRoot;
    expect(root.querySelectorAll('epic-card').length).toBe(1);
    expect(root.querySelectorAll('inbox-item').length).toBe(1);
    root.querySelector('epic-card')?.dispatchEvent(new CustomEvent('stop-epic', { detail: { id: 'ep-1' }, bubbles: true, composed: true }));
    await settle();
    root.querySelector('inbox-item')?.dispatchEvent(new CustomEvent('resolve-item', { detail: { id: 'inc-1', note: 'ok' }, bubbles: true, composed: true }));
    await settle();
    root.querySelector('plan-form')?.dispatchEvent(new CustomEvent('submit-plan', { detail: { text: 'more work please' }, bubbles: true, composed: true }));
    await settle();
    await page.updateComplete;
    expect(root.querySelectorAll('epic-card').length).toBe(1);
    expect(root.querySelectorAll('request-card').length).toBe(1);
    expect(root.querySelector('live-feed')).not.toBeNull();
  });
});

describe('app-shell', () => {
  it('shows the token form when idle and the overview after connecting', async () => {
    const shell = await fixture<AppShell>(html`<app-shell></app-shell>`);
    await settle();
    const root = shell.shadowRoot as ShadowRoot;
    expect(await until(() => root.querySelector('input[name=token]'))).not.toBeNull();
    expect(await until(() => root.querySelector('overview-page'))).not.toBeNull();
    // A stored token connects on load (the live layer, which will fail offline → explained).
    saveToken('abc');
    const again = await fixture<AppShell>(html`<app-shell></app-shell>`);
    await settle();
    expect(['connecting', 'offline', 'online']).toContain((again.shadowRoot as ShadowRoot).querySelector('header')?.className);
    (again.shadowRoot as ShadowRoot).querySelector('button')?.click();
    await again.updateComplete;
    expect(await until(() => (again.shadowRoot as ShadowRoot).querySelector('input[name=token]'))).not.toBeNull();
  });
});

describe('attention drawer, epic page, alerts log', () => {
  it('drawer lists items across rigs and links to the epic', async () => {
    const { attentionOf } = await import('./core/schema.js');
    void attentionOf;
    await import('./components/attention-drawer.js');
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [epic, incident] } }, 'ok');
    const { refreshAll } = await import('./actions.js');
    await refreshAll();
    const el = await fixture<HTMLElement>(html`<attention-drawer></attention-drawer>`);
    const root = el.shadowRoot as ShadowRoot;
    expect(root.querySelector('.badge')?.textContent).toContain('1 needs you');
    (root.querySelector('button') as HTMLButtonElement).click();
    await (el as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    expect((root.querySelector('dialog') as HTMLDialogElement).open).toBe(true);
    expect(root.querySelector('li a')?.getAttribute('href')).toBe('/rigs/toy/epics/ep-1');
  });

  it('epic page shows children, needs-you items, and a timeline', async () => {
    await import('./pages/epic-page.js');
    const { push, reset } = await import('./state/events.js');
    reset();
    const withChildren = { ...epic, metadata: { factory: { ...epic.metadata.factory, children: [{ id: 'ep-1.1', title: 'Do it', state: 'incident', attempts: 3, attemptLimit: 3, tokens: 12000, branch: 'task/ep-1.1', closed: false }] } } };
    connectFake({ token: 'ok', rigs: [rig], tasks: { toy: [withChildren, incident] } }, 'ok');
    push({ rig: 'toy', cursor: 1, replay: false, record: { at: 1, actor: 'worker', bead: 'ep-1.1', kind: 'claimed', holder: 'w-1' } });
    push({ rig: 'toy', cursor: 2, replay: false, record: { at: 2, actor: 'worker', bead: 'zz-1', kind: 'claimed' } });
    const page = await fixture<HTMLElement>(html`<epic-page rig="toy" id="ep-1"></epic-page>`);
    await settle();
    await (page as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    const root = page.shadowRoot as ShadowRoot;
    expect(root.querySelectorAll('tbody tr').length).toBe(1);
    expect(root.textContent).toContain('Do it');
    expect(root.querySelectorAll('inbox-item').length).toBe(1);
    expect(root.querySelectorAll('.timeline li').length).toBe(1);
    await import('./components/alerts-log.js');
    const log = await fixture<HTMLElement>(html`<alerts-log></alerts-log>`);
    expect((log.shadowRoot as ShadowRoot).textContent).toContain('No alerts');
    push({ rig: 'toy', cursor: 3, replay: false, record: { at: 3, actor: 'console', bead: null, kind: 'remote', action: 'alert', detail: 'ep-1 done' } });
    await (log as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    expect((log.shadowRoot as ShadowRoot).querySelectorAll('li').length).toBe(1);
  });
});

describe('epic-page detail sections', () => {
  it('renders the rollup header, plan triptych, and provenance', async () => {
    await import('./pages/epic-page.js');
    const detail = {
      id: 'ep-1', kind: 'epic', title: 'Build it', status: 'open', parent: null,
      description: 'Ship the passthrough.', acceptance: null, task: null, verify: null, notes: [], needs: null,
      context: [
        { id: 'ep-1.0', kind: 'reference', title: 'reference', text: 'Use POSIX sh.' },
        { id: 'c-1', kind: 'contract', title: 'contract: Build it', text: 'range abc..def' },
      ],
      origin: { id: 'pr-1', title: 'Build the passthrough', text: 'Please build it.' },
    };
    const report = {
      epic: 'ep-1', wall_clock: 120, work: 90, parallelism_pct: 75, critical_path: 90, retry_tax: 10,
      first_pass: 1, landed: 2, tokens: 42_000, stages: [], concurrency: [] as Array<[number, number]>, tasks: [],
    };
    connectFake({
      token: 'ok', rigs: [rig], tasks: { toy: [epic] },
      beads: { 'toy/ep-1': detail },
      metrics: { 'toy/ep-1': report },
      consumers: { 'toy/ep-1': [{ rig: 'portal', id: 'pr-9', title: 'Portal after backend', status: 'open' }] },
    }, 'ok');
    const page = await fixture<HTMLElement>(html`<epic-page rig="toy" id="ep-1"></epic-page>`);
    const root = page.shadowRoot as ShadowRoot;
    await until(() => root.querySelector('.rollup'));
    expect(root.querySelector('.rollup')?.textContent).toContain('75%');
    expect(root.querySelector('.rollup')?.textContent).toContain('42k');
    const plans = await until(() => { const p = root.querySelectorAll('details.plan'); return p.length >= 4 ? p : null; });
    const summaries = [...plans].map((d) => d.querySelector('summary')?.textContent ?? '');
    expect(summaries.some((t) => t.includes('Plan text'))).toBe(true);
    expect(summaries.some((t) => t.includes('Reference'))).toBe(true);
    expect(summaries.some((t) => t.includes('Contract'))).toBe(true);
    expect(summaries.some((t) => t.includes('From plan request'))).toBe(true);
    await until(() => root.querySelector('.consumers'));
    expect(root.querySelector('.consumers')?.textContent).toContain('Portal after backend');
  });
});
