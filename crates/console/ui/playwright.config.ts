import { defineConfig } from '@playwright/test';

// End-to-end against `console serve --fake` (no docker, no credentials).
export default defineConfig({
  testDir: 'e2e',
  timeout: 30_000,
  use: { baseURL: process.env['CONSOLE_URL'] ?? 'http://127.0.0.1:7701', headless: true },
  ...(process.env['CONSOLE_URL'] === undefined
    ? {
        webServer: {
          command: 'cargo run -q -p console --features fake -- serve --fake --listen 127.0.0.1:7701 --public-url http://127.0.0.1:7701',
          cwd: '../../..',
          url: 'http://127.0.0.1:7701/.well-known/agent-card.json',
          reuseExistingServer: true,
          timeout: 300_000,
        },
      }
    : {}),
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
});
