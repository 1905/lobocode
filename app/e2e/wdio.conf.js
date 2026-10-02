import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = process.env.LOBO_E2E_ROOT;
if (!root)
  throw new Error(
    "Run through tools/native_app_e2e.py for isolated config and cleanup",
  );
const fixture = JSON.parse(fs.readFileSync(path.join(root, "fixture.json")));
const binary = path.resolve(
  here,
  "../src-tauri/target/debug/bundle/macos/lobocode E2E.app/Contents/MacOS/lobocode",
);
export const config = {
  runner: "local",
  specs: ["./app.spec.js"],
  maxInstances: 1,
  capabilities: [
    { browserName: "tauri", "tauri:options": { application: binary } },
  ],
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: binary,
        driverProvider: "embedded",
        embeddedPort: fixture.port + 1000,
        env: {
          LOBO_E2E_ROOT: root,
          LOBO_APP_CONFIG: fixture.config,
          XDG_STATE_HOME: path.join(root, "state"),
        },
        captureBackendLogs: true,
        captureFrontendLogs: true,
        autoInstallTauriDriver: false,
        autoDownloadEdgeDriver: false,
      },
    ],
  ],
  framework: "mocha",
  reporters: ["spec"],
  logLevel: "warn",
  outputDir: path.join(root, "logs"),
  waitforTimeout: 20000,
  connectionRetryCount: 0,
  mochaOpts: {
    timeout: process.env.LOBO_E2E_CHECK_DRAG === "1" ? 240000 : 90000,
  },
  async afterTest(test, _context, result) {
    const name = test.title.replace(/[^a-z0-9]+/gi, "_");
    try {
      const layout = await browser.execute(async () => ({
        innerWidth,
        innerHeight,
        outerWidth,
        outerHeight,
        nativeSize: await window.__TAURI_INTERNALS__.invoke(
          "plugin:window|inner_size",
          { label: "main" },
        ),
        scale: await window.__TAURI_INTERNALS__.invoke(
          "plugin:window|scale_factor",
          { label: "main" },
        ),
        doc: [
          document.documentElement.clientWidth,
          document.documentElement.clientHeight,
          document.documentElement.scrollWidth,
          document.documentElement.scrollHeight,
        ],
        surface: document
          .querySelector(".surface")
          ?.getBoundingClientRect()
          .toJSON(),
        panel: document.querySelector("main")?.getAttribute("data-phase"),
        body: document.body.innerText,
      }));
      fs.writeFileSync(
        path.join(root, `${name}-layout.json`),
        JSON.stringify(layout, null, 2),
      );
      await browser.saveScreenshot(
        path.join(root, `${name}-${result.passed ? "pass" : "fail"}.png`),
      );
    } catch {
      /* Process may have quit. */
    }
  },
};
