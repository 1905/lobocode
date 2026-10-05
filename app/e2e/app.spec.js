import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const root = process.env.LOBO_E2E_ROOT;
const fixture = JSON.parse(fs.readFileSync(path.join(root, "fixture.json")));
const original = fixture.legacy
  ? fs.readFileSync(fixture.config, "utf8")
  : null;
const ownerPath = fixture.config.replace(/\.env$/, ".app-runtime.json");
const owner = fixture.legacy ? fs.readFileSync(ownerPath, "utf8") : null;
async function invoke(command, args = {}) {
  try {
    return await browser.execute(
      async (command, args) => {
        try {
          return {
            ok: true,
            value: await window.__TAURI_INTERNALS__.invoke(command, args),
          };
        } catch (error) {
          return {
            ok: false,
            error: typeof error === "string" ? error : JSON.stringify(error),
          };
        }
      },
      command,
      args,
    );
  } catch (error) {
    // The embedded driver can surface a rejected IPC as a WebDriver error.
    const message = String(error);
    if (
      !/Command [a-z_]+ not found|Unknown settings tab|cloud providers only|Local model settings are unavailable/.test(
        message,
      )
    )
      throw error;
    return { ok: false, error: message };
  }
}
async function fits() {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const doc = document.documentElement;
        const surface = document
          .querySelector(".surface")
          ?.getBoundingClientRect();
        const controls = [
          ...document.querySelectorAll("button,input,[role=tab]"),
        ].filter((el) => el.getClientRects().length);
        return (
          !!surface &&
          doc.scrollWidth <= doc.clientWidth &&
          doc.scrollHeight <= doc.clientHeight &&
          surface.bottom <= innerHeight + 1 &&
          surface.right <= innerWidth + 1 &&
          controls.every((el) => {
            const r = el.getBoundingClientRect();
            return (
              r.top >= 0 &&
              r.left >= 0 &&
              r.bottom <= innerHeight + 1 &&
              r.right <= innerWidth + 1
            );
          })
        );
      }),
    {
      timeoutMsg:
        "native view and controls must fit without scrolling or clipping",
    },
  );
}
async function click(text) {
  await $(`button*=${text}`).click();
}
async function window(label) {
  await browser.waitUntil(async () =>
    (await browser.getWindowHandles()).includes(label),
  );
  await browser.switchToWindow(label);
}
async function noLocalControls() {
  for (const label of ["weights", "port", "memory", "disk"]) {
    assert.equal(await $(`input[aria-label="${label}"]`).isExisting(), false);
  }
  const labels = await browser.execute(() =>
    [...document.querySelectorAll("button")].map((el) =>
      el.textContent.trim().toLowerCase(),
    ),
  );
  assert.ok(
    !labels.some((label) => /^(\[)?local(\])?$/.test(label)),
    "Local control must not exist",
  );
  await fits();
}

describe("cloud-only native app without provider calls", () => {
  it("opens cloud setup with legacy preferences and preserves CLI data", async () => {
    await window("main");
    await browser.waitUntil(async () => {
      const result = await invoke("get_state");
      return result.ok && result.value.config && result.value.readiness;
    });
    const state = (await invoke("get_state")).value;
    assert.notEqual(state.provider, "local");
    assert.equal(state.model, "q6");
    for (const key of ["target", "is_local", "local_memory", "models"])
      assert.equal(key in state, false);
    assert.equal(state.readiness.cloud_ready, false);
    assert.equal(state.start_allowed, false);
    assert.equal(state.logging_error, null);
    assert.equal(
      state.log_path,
      fixture.config.replace(/\.env$/, ".app-logs/app.jsonl"),
    );
    assert.equal(await $("button*=logs").isExisting(), true);
    assert.equal(fs.statSync(state.log_path).mode & 0o777, 0o600);
    assert.equal(fs.statSync(path.dirname(state.log_path)).mode & 0o777, 0o700);
    const launchLog = fs
      .readFileSync(state.log_path, "utf8")
      .trim()
      .split("\n")
      .map(JSON.parse);
    assert.ok(launchLog.some((entry) => entry.event === "launch"));
    assert.ok(launchLog.some((entry) => entry.event === "config_loaded"));
    assert.equal(await $("button*=START").isExisting(), false);
    await noLocalControls();
    if (fixture.legacy) {
      assert.equal(fs.readFileSync(fixture.config, "utf8"), original);
      assert.equal(fs.readFileSync(ownerPath, "utf8"), owner);
    }
    await browser.saveScreenshot(path.join(root, "native-cloud-setup.png"));
  });

  it("rejects removed local IPC and local settings before mutation", async () => {
    for (const [command, args] of [
      ["choose_target", { v: "local" }],
      ["local_models", {}],
      ["free_bytes", { path: fixture.weights }],
      ["choose_weights", {}],
      ["open_settings", { tab: "local" }],
      ["config_save", { set: { LOBO_PROVIDER: "local" } }],
      ["config_save", { set: { LOBO_WEIGHTS_DIR: fixture.weights } }],
      ["config_save", { set: { LOBO_LOCAL_PORT: "17891" } }],
    ]) {
      const result = await invoke(command, args);
      assert.equal(
        result.ok,
        false,
        `${command} accepted a removed local operation`,
      );
      assert.match(
        result.error,
        /not found|Unknown settings tab|cloud providers only|Local model settings are unavailable/,
      );
    }
    assert.equal((await invoke("set_provider", { v: "local" })).ok, false);
    assert.notEqual((await invoke("get_state")).value.provider, "local");
    assert.equal(
      fs.existsSync(path.join(root, "state/lobo/local.json")),
      false,
    );
    if (fixture.legacy)
      assert.equal(fs.readFileSync(fixture.config, "utf8"), original);
    else assert.equal(fs.existsSync(fixture.config), false);
  });

  it("creates or updates cloud config and fits all settings tabs", async () => {
    await click("SETUP");
    await window("settings");
    await browser.waitUntil(
      async () =>
        (await $('[role="tab"][aria-selected="true"]').getText()) === "Cloud",
    );
    assert.deepEqual(await $$('[role="tab"]').map((el) => el.getText()), [
      "Cloud",
      "Defaults",
      "Clients",
    ]);
    await noLocalControls();
    assert.equal(await $('input[aria-label="cloud port"]').isExisting(), true);
    for (const label of ["bucket url", "domain", "tunnel token"])
      assert.equal(await $(`input[aria-label="${label}"]`).isExisting(), false);
    await $('input[aria-label="cloud port"]').setValue("80");
    await click("SAVE");
    await browser.waitUntil(async () =>
      (await $('[role="status"]').getText()).includes("1024"),
    );
    await $('input[aria-label="cloud port"]').setValue(String(fixture.port));
    await click("Defaults");
    await noLocalControls();
    await $('input[aria-label="context"]').setValue("16384");
    await click("q6");
    await click("SAVE");
    await browser.waitUntil(async () =>
      (await $('[role="status"]').getText()).startsWith("saved "),
    );
    const saved = fs.readFileSync(fixture.config, "utf8");
    assert.match(saved, /^LOBO_API_KEY=sk-/m);
    assert.match(saved, /^LOBO_CTX=16384$/m);
    assert.match(saved, /^LOBO_CONNECTION=ssh$/m);
    assert.equal(fs.statSync(fixture.config).mode & 0o777, 0o600);
    const diagnosticState = (await invoke("get_state")).value;
    const diagnosticLog = fs.readFileSync(diagnosticState.log_path, "utf8");
    const apiKey = saved.match(/^LOBO_API_KEY=(.+)$/m)?.[1];
    assert.ok(apiKey);
    assert.ok(
      !diagnosticLog.includes(apiKey),
      "diagnostics must not retain the API key",
    );
    if (fixture.legacy) {
      assert.match(saved, /^LOBO_PROVIDER=local$/m);
      assert.match(saved, /^LOBO_LOCAL_PORT=17891$/m);
      assert.ok(saved.includes(`LOBO_WEIGHTS_DIR=${fixture.weights}`));
      assert.equal(fs.readFileSync(ownerPath, "utf8"), owner);
    }
    await browser.saveScreenshot(
      path.join(root, "native-settings-defaults.png"),
    );
    await click("Clients");
    await noLocalControls();
    await browser.saveScreenshot(
      path.join(root, "native-settings-clients.png"),
    );
    await click("Cloud");
    await noLocalControls();
    await browser.saveScreenshot(path.join(root, "native-settings-cloud.png"));
    await browser.closeWindow();
    await window("main");
    await noLocalControls();
    assert.equal(await $("button*=START").isExisting(), false);
    if (process.env.LOBO_E2E_CHECK_DRAG === "1") {
      const before = await browser.getWindowRect();
      fs.writeFileSync(
        path.join(root, "drag-ready.json"),
        JSON.stringify(before),
      );
      await browser.waitUntil(
        async () => {
          const after = await browser.getWindowRect();
          if (Math.abs(after.x - before.x) + Math.abs(after.y - before.y) < 40)
            return false;
          fs.writeFileSync(
            path.join(root, "drag-passed.json"),
            JSON.stringify({ before, after }),
          );
          return true;
        },
        {
          timeout: 180000,
          timeoutMsg: "native title-bar drag did not move the window",
        },
      );
    }
  });
});
