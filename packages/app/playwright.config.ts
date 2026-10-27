import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests run against the `mire serve` binary, which embeds the
 * production web build. Each test starts its own server through the fixture in
 * `tests/mire.ts`, so there is no shared web server here. Build the web app and
 * then the binary first; `just e2e` does both.
 */
export default defineConfig({
	testDir: 'tests',
	testMatch: '**/*.e2e.{ts,js}',
	forbidOnly: !!process.env.CI,
	retries: 0,
	reporter: process.env.CI ? [['list'], ['github']] : 'list',
	use: {
		trace: 'retain-on-failure',
		launchOptions: { executablePath: process.env.MIRE_E2E_CHROMIUM || undefined }
	},
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }]
});
