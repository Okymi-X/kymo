import { defineConfig } from '@playwright/test'
import { PASSWORD } from './e2e/constants.js'

const PORT = 8791

export default defineConfig({
  testDir: 'e2e',
  fullyParallel: false,
  workers: 1,
  // One retry on CI absorbs a rare timing hiccup on a shared runner without hiding real failures.
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['list']] : 'list',
  use: { baseURL: `http://127.0.0.1:${PORT}`, channel: 'chrome', trace: 'retain-on-failure' },
  webServer: {
    command: 'rm -f target/e2e.db* && cargo run -q -p kymo-server',
    cwd: '..',
    url: `http://127.0.0.1:${PORT}/api/v1/tournaments`,
    // A cold CI build of the server takes a while before it can answer.
    timeout: process.env.CI ? 900_000 : 180_000,
    reuseExistingServer: false,
    env: {
      KYMO_ADMIN_PASSWORD: PASSWORD,
      KYMO_ADDR: `127.0.0.1:${PORT}`,
      KYMO_DATABASE_URL: 'sqlite://target/e2e.db',
    },
  },
})
