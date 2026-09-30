import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const root = process.env.LOBO_E2E_ROOT;
const fixture = JSON.parse(fs.readFileSync(path.join(root, "fixture.json")));
const memoryFile = path.join(root, "memory.json");
const markerFile = path.join(root, "memory-read.json");
const stateRoot = path.join(root, "state");
const evidence = [];

function mode(mode, options = {}) {
  // Atomic replacement prevents the native reader from seeing partial JSON.
  const temporary = `${memoryFile}.next`;
  fs.writeFileSync(temporary, JSON.stringify({ mode, ...options }));
  fs.renameSync(temporary, memoryFile);
}
async function ipc(command, args = {}) {
  return browser.execute(
    (command, args) => window.__TAURI_INTERNALS__.invoke(command, args),
    command,
    args,
  );
}
async function fits() {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const doc = document.documentElement;
        const surface = document
          .querySelector(".surface")
          ?.getBoundingClientRect();
        return (
          !!surface &&
          doc.scrollWidth <= doc.clientWidth &&
          doc.scrollHeight <= doc.clientHeight &&
          surface.bottom <= innerHeight + 1 &&
          surface.right <= innerWidth + 1 &&
          [...document.querySelectorAll("main.panel button")].every(
            (button) => {
              const bounds = button.getBoundingClientRect();
              return (
                bounds.top >= 0 &&
                bounds.left >= 0 &&
                bounds.bottom <= innerHeight + 1 &&
                bounds.right <= innerWidth + 1
              );
            },
          )
        );
      }),
    { timeoutMsg: "memory controls must stay visible without scrolling" },
  );
}
async function verdict(status, model, ctx) {
  await browser.waitUntil(
    async () => {
      const state = await ipc("get_state");
      return (
        state.local_memory?.status === status &&
        state.local_memory?.model === model &&
        (ctx === undefined || state.local_memory?.ctx === ctx)
      );
    },
    { timeoutMsg: `expected ${model} memory ${status} at context ${ctx}` },
  );
  assert.equal(await $("[data-memory]").getAttribute("data-memory"), status);
  await fits();
  const state = await ipc("get_state");
  evidence.push({
    case: `${model}-${status}-${ctx ?? state.local_memory.ctx}`,
    memory: state.local_memory,
  });
  fs.writeFileSync(
    path.join(root, "memory-evidence.json"),
    JSON.stringify(evidence, null, 2),
  );
  return state.local_memory;
}
async function click(text) {
  await $(`button*=${text}`).click();
}
function files(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .flatMap((entry) => {
      const file = path.join(dir, entry.name);
      return entry.isDirectory() ? files(file) : [file];
    })
    .sort();
}
function noRuntime() {
  for (const file of files(stateRoot)) {
    assert.ok(
      !/\/(?:local\.json|local\.log|operation\.json|.*\.pid)$/.test(file),
      `unexpected runtime artifact ${file}`,
    );
  }
  assert.equal(
    fs.existsSync(path.join(fixture.weights, ".e2e-llama.pid")),
    false,
  );
  assert.equal(
    fs.existsSync(path.join(fixture.weights, "runtime")),
    false,
    "denied Start must not download a runtime",
  );
}

describe("native memory guard (UI only; no passing Start)", () => {
  it("shows Q8 insufficient memory and disables keyboard and button Start", async () => {
    await browser.waitUntil(async () =>
      (await browser.getWindowHandles()).includes("main"),
    );
    await browser.switchToWindow("main");
    await browser.waitUntil(
      async () => (await ipc("get_state")).phase.kind === "off",
    );
    await click("local");
    await click("q8");
    const memory = await verdict("insufficient", "q8", 8192);
    assert.ok(memory.required_bytes > memory.budget_bytes);
    assert.match(
      await $("[data-memory]").getText(),
      /GiB required.*GiB budget/,
    );
    assert.match(
      await $("[data-memory]").getText(),
      /Close other apps or use Cloud/,
    );
    assert.equal(await $("button*=START").isEnabled(), false);
    await browser.execute(() =>
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" })),
    );
    assert.equal((await ipc("get_state")).phase.kind, "off");
    noRuntime();
  });

  it("refreshes Q6 separately without changing model or target automatically", async () => {
    mode("q6_only");
    await ipc("refresh", { models: false });
    await verdict("insufficient", "q8");
    assert.equal((await ipc("get_state")).model, "q8");
    await click("q6");
    await verdict("ready", "q6", 8192);
    assert.equal(await $("button*=START").isEnabled(), true);
    assert.match(await $("[data-memory]").getText(), /Memory check passed/);
    noRuntime();
  });

  it("shows unavailable measurements and keeps explicit Cloud selection usable", async () => {
    mode("unavailable", {
      message: "native probe unavailable: counter read failed. ".repeat(40),
    });
    await click("q8");
    const memory = await verdict("unavailable", "q8");
    assert.equal(memory.required_bytes, null);
    assert.equal(memory.budget_bytes, null);
    assert.equal(await $("button*=START").isEnabled(), false);
    assert.match(await $("[data-memory]").getText(), /probe unavailable/);
    await click("cloud");
    await browser.waitUntil(
      async () => (await ipc("get_state")).target === "cloud",
    );
    assert.equal((await ipc("get_state")).local_memory, null);
    assert.equal(await $("[data-memory]").isExisting(), false);
    assert.equal(await $("button*=SETUP").isEnabled(), true);
    await fits();
    noRuntime();
  });

  it("displays enough memory and refreshes a saved context without starting", async () => {
    mode("ready");
    await click("local");
    const before = await verdict("ready", "q8", 8192);
    await click("settings");
    await browser.waitUntil(async () =>
      (await browser.getWindowHandles()).includes("settings"),
    );
    await browser.switchToWindow("settings");
    await click("Defaults");
    await $('input[aria-label="context"]').setValue("65536");
    await click("q8");
    await click("SAVE");
    await browser.waitUntil(async () =>
      (await $('[role="status"]').getText()).startsWith("saved "),
    );
    await fits();
    await browser.closeWindow();
    await browser.switchToWindow("main");
    const after = await verdict("ready", "q8", 65536);
    assert.ok(after.required_bytes > before.required_bytes);
    noRuntime();
  });

  it("rejects a delayed stale pass after changing the selected model", async () => {
    mode("ready", { delay_ms: 1500, token: "stale-q8" });
    await ipc("set_model", { v: "q8" });
    await browser.waitUntil(
      () =>
        fs.existsSync(markerFile) &&
        JSON.parse(fs.readFileSync(markerFile)).token === "stale-q8",
    );
    assert.equal(
      await $("[data-memory]").getAttribute("data-memory"),
      "checking",
    );
    assert.equal(await $("button*=START").isEnabled(), false);
    mode("insufficient", { token: "fresh-q6" });
    await click("q6");
    await verdict("insufficient", "q6", 65536);
    // The hook delay is bounded. Waiting beyond it lets the captured pass return.
    await browser.pause(1700);
    await verdict("insufficient", "q6", 65536);
    assert.equal(await $("button*=START").isEnabled(), false);
    noRuntime();
  });

  it("fresh real Start IPC rejects a displayed pass and Retry without artifacts", async () => {
    mode("ready");
    await click("q8");
    await verdict("ready", "q8", 65536);
    const beforeWeights = files(fixture.weights);
    const beforeState = files(stateRoot);
    // Replace the passing display fixture before invoking Start. The current
    // displayed pass cannot authorize the fresh denying backend snapshot.
    mode("insufficient");
    await browser.execute(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__loboStartError = null;
      window.__TAURI_INTERNALS__.invoke = async function (command, args) {
        try {
          return await invoke.call(this, command, args);
        } catch (error) {
          if (command === "start") window.__loboStartError = error;
          throw error;
        }
      };
    });
    await click("START");
    await browser.waitUntil(async () =>
      browser.execute(() => !!window.__loboStartError),
    );
    assert.match(
      (await browser.execute(() => window.__loboStartError)).message,
      /requires .* GiB/,
    );
    await verdict("insufficient", "q8", 65536);
    assert.equal((await ipc("get_state")).phase.kind, "failed");
    assert.equal(await $("button*=RETRY").isEnabled(), false);
    assert.equal(
      await $(".body > p.error").isExisting(),
      false,
      "phase already contains the Start error",
    );
    for (let attempt = 0; attempt < 2; attempt++) {
      const error = await browser.execute(async () => {
        try {
          await window.__TAURI_INTERNALS__.invoke("start");
          return null;
        } catch (error) {
          return error;
        }
      });
      assert.ok(error, "forced-denied Start must return an IPC error");
      assert.match(
        error.message,
        /q8 with 65536 context tokens requires .* GiB/,
      );
      assert.equal((await ipc("get_state")).phase.kind, "failed");
      await verdict("insufficient", "q8", 65536);
      await fits();
      noRuntime();
      assert.deepEqual(files(fixture.weights), beforeWeights);
      assert.deepEqual(files(stateRoot), beforeState);
    }
  });

  it("restores the real native read-only probe without invoking Start", async () => {
    mode("real", { token: "native-read-only" });
    await ipc("dismiss");
    await ipc("refresh", { models: false });
    const state = await ipc("get_state");
    const memory = state.local_memory;
    assert.ok(memory);
    assert.equal(memory.model, "q8");
    assert.equal(memory.ctx, 65536);
    assert.ok(["ready", "insufficient", "unavailable"].includes(memory.status));
    if (memory.status !== "unavailable") {
      assert.ok(memory.total_bytes > 0);
      assert.ok(
        memory.available_bytes >= 0 &&
          memory.available_bytes <= memory.total_bytes,
      );
      assert.ok(memory.metal_limit_bytes > 0);
    }
    await verdict(memory.status, "q8", 65536);
    assert.equal(
      JSON.parse(fs.readFileSync(markerFile)).token,
      "native-read-only",
    );
    noRuntime();
  });
});
