// The single route table. CSR only: every route is a lazily-loaded page element.
import { html, type TemplateResult } from 'lit';

export interface Params {
  readonly [k: string]: string | undefined;
}

export interface Route {
  readonly path: string;
  readonly enter: () => Promise<unknown>;
  readonly template: (params: Params) => TemplateResult;
  readonly title: (params: Params) => string;
}

export const routes: ReadonlyArray<Route> = [
  {
    path: '/',
    enter: () => import('./pages/overview-page.js'),
    template: () => html`<overview-page></overview-page>`,
    title: () => 'Factory — rigs',
  },
  {
    path: '/rigs/:rig',
    enter: () => import('./pages/rig-page.js'),
    template: (p) => html`<rig-page .rig=${p['rig'] ?? ''}></rig-page>`,
    title: (p) => `Factory — ${p['rig'] ?? ''}`,
  },
  {
    path: '/rigs/:rig/epics/:id',
    enter: () => import('./pages/epic-page.js'),
    template: (p) => html`<epic-page .rig=${p['rig'] ?? ''} .id=${p['id'] ?? ''}></epic-page>`,
    title: (p) => `Factory — ${p['id'] ?? ''}`,
  },
  {
    path: '/rigs/:rig/epics/:id/throughput',
    enter: () => import('./pages/throughput-page.js'),
    template: (p) => html`<throughput-page .rig=${p['rig'] ?? ''} .id=${p['id'] ?? ''}></throughput-page>`,
    title: (p) => `Factory — ${p['id'] ?? ''} throughput`,
  },
];

export const matchRoute = (url: URL): { readonly route: Route; readonly params: Params } | null => {
  for (const route of routes) {
    const m = new URLPattern({ pathname: route.path }).exec(url);
    if (m) return { route, params: m.pathname.groups };
  }
  return null;
};
