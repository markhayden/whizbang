// The Chrome tier (plan m5, "Where the Chrome tier runs"; ADR 0043): Playwright's runner
// driving the Google Chrome installed on the machine (`channel: "chrome"`; no browser is
// downloaded), one worker, no retry, 30 seconds a case. `scripts/e2e.sh` builds the fixture
// host, runs this, and holds the run to 240 seconds of test time from the JSON report.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  testMatch: "**/*.spec.ts",
  workers: 1,
  fullyParallel: false,
  retries: 0,
  timeout: 30_000,
  expect: { timeout: 10_000 },
  forbidOnly: true,
  outputDir: "../target/e2e/results",
  reporter: [["list"], ["json", { outputFile: "../target/e2e/report.json" }], ["html", { outputFolder: "../target/e2e/html", open: "never" }]],
  use: {
    channel: "chrome",
    headless: true,
    viewport: { width: 1440, height: 900 },
    trace: "retain-on-failure",
    launchOptions: {
      // A name a hostile page could point at loopback (security.spec.ts): resolved by
      // Chrome alone, so the machine's own resolver is not touched.
      args: ["--host-resolver-rules=MAP rebind.test 127.0.0.1"],
    },
  },
});
