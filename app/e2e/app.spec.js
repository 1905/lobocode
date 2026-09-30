import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";

const root = process.env.LOBO_E2E_ROOT;
const fixture = JSON.parse(fs.readFileSync(path.join(root, "fixture.json")));
const stateFile = path.join(root, "state/lobo/local.json");
const modeFile = path.join(fixture.weights, ".e2e-mode.json");
const apiKey = () =>
  fs.readFileSync(fixture.config, "utf8").match(/^LOBO_API_KEY=(.*)$/m)[1];
async function phase(word) {
  const kinds = {
    SETUP: "no_config",
    OFF: "off",
    RUN: "ready",
    BOOT: "booting",
    FAIL: "failed",
  };
  await browser.waitUntil(
    async () =>
      (await $("main.panel").getAttribute("data-phase")) === kinds[word],
    { timeoutMsg: `expected app phase ${word}` },
  );
  await fits();
}
async function fits() {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const doc = document.documentElement;
        const surface = document
          .querySelector(".surface")
          .getBoundingClientRect();
        return (
          doc.scrollWidth <= doc.clientWidth &&
          doc.scrollHeight <= doc.clientHeight &&
          surface.bottom <= innerHeight + 1 &&
          surface.right <= innerWidth + 1
        );
      }),
    { timeoutMsg: "native view must fit without scrolling or clipping" },
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
async function gone(pid) {
  await browser.waitUntil(
    () => {
      try {
        process.kill(pid, 0);
        return false;
      } catch (e) {
        if (e.code === "ESRCH") return true;
        throw e;
      }
    },
    { timeout: 20000, timeoutMsg: `owned process ${pid} survived Stop` },
  );
}

describe("native app with real Rust core and isolated local runtime", () => {
  it("creates config through SETUP and validates settings", async () => {
    await window("main");
    await phase("SETUP");
    await click("SETUP");
    await window("settings");
    await fits();
    await $('input[aria-label="weights"]').setValue(fixture.weights);
    await $('input[aria-label="port"]').setValue("80");
    await click("SAVE");
    await browser.waitUntil(async () =>
      (await $('[role="status"]').getText()).includes("1024-65534"),
    );
    assert.equal(fs.existsSync(fixture.config), false);
    await $('input[aria-label="port"]').setValue(String(fixture.port));
    await click("Defaults");
    await fits();
    await $('input[aria-label="context"]').setValue("8192");
    await click("q6");
    await click("SAVE");
    await browser.waitUntil(async () =>
      (await $('[role="status"]').getText()).startsWith("saved "),
    );
    assert.match(apiKey(), /^sk-/);
    assert.equal(fs.statSync(fixture.config).mode & 0o777, 0o600);
    await click("Cloud");
    await fits();
    await click("Local");
    await fits();
    await browser.closeWindow();
    await window("main");
    await phase("OFF");
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

  it("starts the real supervisor, copies values and stops every child", async () => {
    await click("START");
    await phase("RUN");
    const state = JSON.parse(fs.readFileSync(stateFile));
    const command = execFileSync(
      "ps",
      ["-ww", "-o", "command=", "-p", String(state.pid)],
      { encoding: "utf8" },
    );
    assert.ok(command.includes("--lobo-local-run local run"));
    assert.ok(command.includes(fixture.config));
    assert.ok(command.includes(state.boot_id));
    const response = await fetch(`http://127.0.0.1:${fixture.port}/v1/models`, {
      headers: { Authorization: `Bearer ${apiKey()}` },
    });
    assert.equal(response.status, 200);
    assert.equal(
      (await response.json()).data[0].id,
      "qwen3.5-27b-uncensored-q6",
    );
    const values = await $$(".value");
    const oldClipboard = execFileSync("pbpaste");
    try {
      await values[0].$("button").click();
      assert.equal(
        execFileSync("pbpaste", { encoding: "utf8" }),
        `http://127.0.0.1:${fixture.port}/v1`,
      );
      await values[1].$("button").click();
      assert.equal(execFileSync("pbpaste", { encoding: "utf8" }), apiKey());
    } finally {
      execFileSync("pbcopy", [], { input: oldClipboard });
    }
    const child = Number(
      fs.readFileSync(path.join(fixture.weights, ".e2e-llama.pid"), "utf8"),
    );
    await click("STOP");
    await phase("OFF");
    await gone(state.pid);
    await gone(child);
    assert.equal(fs.existsSync(stateFile), false);
  });

  it("aborts during runtime loading without leaving a process", async () => {
    fs.writeFileSync(modeFile, JSON.stringify({ health_delay: 60 }));
    await click("START");
    await phase("BOOT");
    await browser.waitUntil(() => fs.existsSync(stateFile));
    const state = JSON.parse(fs.readFileSync(stateFile));
    await click("ABORT");
    await phase("OFF");
    await gone(state.pid);
    assert.equal(fs.existsSync(stateFile), false);
    fs.writeFileSync(modeFile, "{}");
  });

  it("shows a runtime failure and allows retry", async () => {
    fs.writeFileSync(modeFile, JSON.stringify({ exit: true }));
    await click("START");
    await phase("FAIL");
    assert.equal(fs.existsSync(stateFile), false);
    fs.writeFileSync(modeFile, "{}");
    await click("RETRY");
    await phase("RUN");
    await click("STOP");
    await phase("OFF");
  });
});
