import { defineConfig, devices } from "@playwright/test";

// Visual regression tests: full-page screenshots of the main views on desktop
// and mobile viewports, compared against committed baselines. Baselines are
// rendered inside the Playwright Docker image (see .github/workflows/ci.yml)
// so fonts match CI; regenerate them there with `--update-snapshots`.
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://localhost:8123",
  },
  expect: {
    toHaveScreenshot: {
      animations: "disabled",
      caret: "hide",
      stylePath: "e2e/screenshot.css",
    },
  },
  projects: [
    {
      name: "desktop",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } },
    },
    {
      name: "mobile",
      use: { ...devices["Pixel 7"] },
    },
  ],
  webServer: {
    command: "bash e2e/serve.sh",
    url: "http://localhost:8123/healthcheck",
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
  },
});
