import { LitElement, html, css } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { SignalWatcher } from '@lit-labs/signals';
import { lastRefreshAt, refreshAll } from './actions.js';
import { startLive, stopLive } from './live.js';
import { streamStatus, type StreamStatus } from './state/events.js';

/** Poll every 15 s without a live stream, every 90 s with one. */
export const backstopDue = (stream: StreamStatus, last: number, now: number): boolean =>
  now - last >= (stream === 'live' ? 90_000 : 15_000);
import { connect, disconnect } from './core/runtime.js';
import { Router } from './router.js';
import './components/attention-drawer.js';
import { baseUrl, connection, loadToken, saveToken, token } from './state/session.js';
import { controls, badges } from './styles/shared.js';
import './components/toast-stack.js';
import './components/error-panel.js';

@customElement('app-shell')
export class AppShell extends SignalWatcher(LitElement) {
  static override styles = [controls, badges, css`
    :host { display: grid; grid-template-rows: auto 1fr; min-block-size: 100dvh; }
    header {
      position: sticky; inset-block-start: 0; z-index: 10;
      display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap;
      padding: var(--space-3) var(--space-6);
      background: var(--bg-elev); backdrop-filter: blur(16px) saturate(1.4); border-block-end: 1px solid var(--line);
    }
    .brand { font-weight: 900; letter-spacing: -0.02em; font-size: 1.15rem; text-decoration: none; color: inherit; display: inline-flex; gap: .4rem; align-items: center; }
    .brand::before { content: ''; inline-size: .8rem; block-size: .8rem; border-radius: 3px; background: linear-gradient(135deg, var(--accent), var(--ok)); rotate: 45deg; }
    form { display: flex; gap: var(--space-2); align-items: center; margin-inline-start: auto; }
    form input { inline-size: 16rem; }
    .dot { inline-size: .6rem; block-size: .6rem; border-radius: 50%; background: var(--fg-muted); }
    .online .dot { background: var(--ok); box-shadow: 0 0 0 4px color-mix(in oklch, var(--ok) 25%, transparent); }
    .connecting .dot { background: var(--warn); animation: pulse 1s infinite; }
    .offline .dot { background: var(--danger); }
    .status { display: inline-flex; gap: .5rem; align-items: center; font-size: .85rem; color: var(--fg-muted); }
    .stream.live { color: var(--ok); font-weight: 700; } .stream.reconnecting, .stream.connecting { color: var(--warn); }
    main { padding: var(--space-6); max-inline-size: 80rem; inline-size: 100%; margin-inline: auto; }
    @keyframes pulse { 50% { opacity: .4; } }
  `];

  private readonly router = new Router(this);
  @state() private draft = '';
  private timer: number | undefined;

  override connectedCallback(): void {
    super.connectedCallback();
    this.draft = loadToken();
    if (this.draft !== '') this.start(this.draft);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this.timer !== undefined) clearInterval(this.timer);
  }

  private start(tok: string): void {
    saveToken(tok);
    connect({ baseUrl: baseUrl.get(), token: tok });
    void refreshAll();
    startLive(baseUrl.get(), tok);
    if (this.timer !== undefined) clearInterval(this.timer);
    // The stream drives refreshes; the timer is a backstop: 90 s while live, 15 s otherwise.
    this.timer = window.setInterval(() => { if (backstopDue(streamStatus.get(), lastRefreshAt.get(), Date.now())) void refreshAll(); }, 15_000);
  }

  private stop(): void {
    if (this.timer !== undefined) clearInterval(this.timer);
    stopLive();
    disconnect();
    connection.set('idle');
    saveToken('');
    this.draft = '';
  }

  override render() {
    const conn = connection.get();
    return html`
      <header class=${conn}>
        <a class="brand" href="/">factory</a>
        <attention-drawer></attention-drawer>
        <span class="status" aria-live="polite"><span class="dot" aria-hidden="true"></span>${conn}${conn === 'online' ? html` · <span class="stream ${streamStatus.get()}">${streamStatus.get() === 'live' ? 'live' : streamStatus.get()}</span>` : ''}</span>
        <form @submit=${this.onConnect}>
          ${token.get() === '' || conn === 'idle'
            ? html`<label>Token <input name="token" type="password" autocomplete="off" required .value=${this.draft} @input=${(e: Event) => { this.draft = (e.target as HTMLInputElement).value; }}></label>
                   <button type="submit" class="primary">Connect</button>`
            : html`<button type="button" @click=${() => { this.stop(); }}>Disconnect</button>`}
        </form>
      </header>
      <main id="main">
        <error-panel @recover=${this.onRecover}></error-panel>
        ${this.router.outlet}
      </main>
      <toast-stack></toast-stack>`;
  }

  private readonly onRecover = (e: CustomEvent<{ action: string }>): void => {
    if (e.detail.action === 'retry') { void refreshAll(); return; }
    this.stop();
    void this.updateComplete.then(() => { (this.shadowRoot?.querySelector('input[name=token]') as HTMLInputElement | null)?.focus(); });
  };

  private readonly onConnect = (e: Event): void => {
    e.preventDefault();
    if (this.draft.trim() === '') return;
    this.start(this.draft.trim());
  };
}

declare global {
  interface HTMLElementTagNameMap { 'app-shell': AppShell; }
}
