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
    expect(root.querySelectorAll('epic-card').length).toBe(2);
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
