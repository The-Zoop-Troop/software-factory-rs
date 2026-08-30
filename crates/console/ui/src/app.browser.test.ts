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

beforeEach(() => { resetRigs(); resetSession(); disconnect(); localStorage.clear(); history.replaceState(null, '', '/'); });

describe('routes', () => {
  it('match the overview and a rig', () => {
    expect(matchRoute(new URL('http://x/'))?.route.path).toBe('/');
    expect(matchRoute(new URL('http://x/rigs/toy'))?.params['rig']).toBe('toy');
    expect(matchRoute(new URL('http://x/nope/x'))).toBeNull();
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
    expect(root.querySelector('input[name=token]')).not.toBeNull();
    expect(root.querySelector('overview-page')).not.toBeNull();
    // A stored token connects on load (the live layer, which will fail offline → explained).
    saveToken('abc');
    const again = await fixture<AppShell>(html`<app-shell></app-shell>`);
    await settle();
    expect(['connecting', 'offline', 'online']).toContain((again.shadowRoot as ShadowRoot).querySelector('header')?.className);
    (again.shadowRoot as ShadowRoot).querySelector('button')?.click();
    await again.updateComplete;
    expect((again.shadowRoot as ShadowRoot).querySelector('input[name=token]')).not.toBeNull();
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
    push({ rig: 'toy', cursor: 1, record: { at: 1, actor: 'worker', bead: 'ep-1.1', kind: 'claimed', holder: 'w-1' } });
    push({ rig: 'toy', cursor: 2, record: { at: 2, actor: 'worker', bead: 'zz-1', kind: 'claimed' } });
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
    push({ rig: 'toy', cursor: 3, record: { at: 3, actor: 'console', bead: null, kind: 'remote', action: 'alert', detail: 'ep-1 done' } });
    await (log as unknown as { updateComplete: Promise<boolean> }).updateComplete;
    expect((log.shadowRoot as ShadowRoot).querySelectorAll('li').length).toBe(1);
  });
});
