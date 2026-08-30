// Platform-native router: URLPattern + Navigation API, click/popstate fallback.
import type { ReactiveController, ReactiveControllerHost } from 'lit';
import { html, type TemplateResult } from 'lit';
import { matchRoute } from './routes.js';

interface NavigateEventLike {
  readonly canIntercept: boolean;
  readonly hashChange: boolean;
  readonly downloadRequest: string | null;
  readonly destination: { readonly url: string };
  intercept(opts: { handler: () => Promise<void> }): void;
}

interface NavigationLike {
  addEventListener(type: 'navigate', cb: (e: NavigateEventLike) => void): void;
}

export class Router implements ReactiveController {
  outlet: TemplateResult = html``;

  constructor(private readonly host: ReactiveControllerHost & HTMLElement) {
    host.addController(this);
  }

  hostConnected(): void {
    const nav = (window as unknown as { navigation?: NavigationLike }).navigation;
    if (nav !== undefined) {
      nav.addEventListener('navigate', (e) => {
        if (!e.canIntercept || e.hashChange || e.downloadRequest !== null) return;
        const url = new URL(e.destination.url);
        if (url.origin !== location.origin || !matchRoute(url)) return;
        e.intercept({ handler: () => this.show(url) });
      });
    } else {
      document.addEventListener('click', this.onClick);
      window.addEventListener('popstate', () => {
        void this.show(new URL(location.href));
      });
    }
    void this.show(new URL(location.href));
  }

  async show(url: URL): Promise<void> {
    const match = matchRoute(url);
    if (!match) {
      this.outlet = html`<section class="empty"><h1>Not found</h1><p><a href="/">Back to rigs</a></p></section>`;
      this.host.requestUpdate();
      return;
    }
    await match.route.enter();
    const apply = (): void => {
      this.outlet = match.route.template(match.params);
      document.title = match.route.title(match.params);
      this.host.requestUpdate();
    };
    interface Transition { readonly finished: Promise<void>; readonly ready: Promise<void>; readonly updateCallbackDone: Promise<void> }
    const doc = document as Document & { startViewTransition?: (cb: () => void) => Transition };
    if (typeof doc.startViewTransition === 'function') {
      // A transition is skipped when another starts or the tab is hidden; that is not an error.
      const t = doc.startViewTransition(apply);
      t.finished.catch(() => undefined);
      t.ready.catch(() => undefined);
      t.updateCallbackDone.catch(() => undefined);
    } else {
      apply();
    }
  }

  private readonly onClick = (e: MouseEvent): void => {
    const a = e.composedPath().find((el) => (el as Element).localName === 'a') as HTMLAnchorElement | undefined;
    if (!a || a.origin !== location.origin || a.target !== '' || e.metaKey || e.ctrlKey) return;
    if (!matchRoute(new URL(a.href))) return;
    e.preventDefault();
    history.pushState(null, '', a.href);
    void this.show(new URL(a.href));
  };
}
