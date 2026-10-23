import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { click, fits } from "./native-helpers.js";

const root = process.env.LOBO_E2E_ROOT;
const fixture = JSON.parse(fs.readFileSync(path.join(root, "fixture.json")));
assert.equal(fixture.mode, "runtime-ui-only");
const config = path.join(root, "config-home/opencode/opencode.jsonc");
const ownerFile = path.join(root, "config.app-runtime.json");
const pendingFile = path.join(root, "state/lobo/operation.json");
const pendingBefore = fs.readFileSync(pendingFile);
const alias = JSON.parse(fs.readFileSync(path.resolve("../../crates/lobo-proto/catalog.json")))
  .find((model) => model.id === "q6").alias;
const acceptance = Object.fromEntries([
  "dock", "cmd_tab", "main_drag", "settings_drag", "rounded_corners", "tray_toggle", "keyboard_focus", "quit", "native_file_chooser",
].map((name) => [name, "pending native acceptance"]));
function writeAcceptance() {
  fs.writeFileSync(path.join(root, "native-acceptance.json"), JSON.stringify(acceptance, null, 2));
}
async function ipc(command, args = {}) {
  return browser.execute((command, args) => window.__TAURI_INTERNALS__.invoke(command, args), command, args);
}
async function window(label) {
  await browser.waitUntil(async () => (await browser.getWindowHandles()).includes(label));
  await browser.switchToWindow(label);
  await visible(label, true);
}
async function visible(label, expected) {
  await browser.waitUntil(async () => await ipc("plugin:window|is_visible", { label }) === expected,
    { timeoutMsg: `native ${label} visibility did not become ${expected}` });
}
function nativeTrace() {
  const file = path.join(root, "native-trace.jsonl");
  return fs.existsSync(file) ? fs.readFileSync(file, "utf8").trim().split("\n").filter(Boolean).map(JSON.parse) : [];
}
async function phase(kind) {
  await browser.waitUntil(async () => (await ipc("get_state")).phase.kind === kind,
    { timeoutMsg: `expected runtime fixture phase ${kind}` });
  assert.equal(await $("main.panel").getAttribute("data-phase"), kind);
  await fits();
}
async function request(route, body) {
  const response = await fetch(fixture.runtime_origin + route, {
    method: body ? "POST" : "GET",
    headers: { "X-Lobo-Fixture-Control": fixture.control_token, "Content-Type": "application/json" },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  assert.equal(response.status, 200, "owned fixture request failed");
  return response.json();
}
const control = (body) => request("/fixture/control", body);
const evidence = () => request("/fixture/evidence");
function inventory(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    // Assertion diffs must never print private key bytes.
    return entry.isDirectory() ? inventory(file) : [{ file, mode: fs.statSync(file).mode & 0o777,
      digest: createHash("sha256").update(fs.readFileSync(file)).digest("hex") }];
  }).sort((a, b) => a.file.localeCompare(b.file));
}
function privateFiles() {
  return [...inventory(path.join(root, "opencode-keys")), ...inventory(path.join(root, "config-home/opencode"))];
}
function unchanged(before) { assert.deepEqual(privateFiles(), before); }
async function noCloud() {
  const observed = await evidence();
  assert.equal(Object.entries(observed.counts).filter(([name]) => /^(runpod|vast)\./.test(name))
    .reduce((count, [, n]) => count + n, 0), 0, "Local UI/Stop contacted a cloud adapter");
  assert.equal(observed.counts.forbidden_generation || 0, 0);
  assert.equal(observed.counts.forbidden_provider_method || 0, 0);
  assert.deepEqual(fs.readFileSync(pendingFile), pendingBefore, "foreign pending operation changed");
}
async function clients() {
  await window("main");
  await click("OpenCode…");
  await window("settings");
  await browser.waitUntil(async () => await $("#section-clients .path").getText() === config);
  await fits();
}
async function repair() {
  const before = (await evidence()).counts["models.request"] || 0;
  await click("Configure/Repair");
  await browser.waitUntil(async () => ((await evidence()).counts["models.request"] || 0) > before);
  await browser.waitUntil(async () => await $("button*=Configure/Repair").isExisting());
}
async function stop() {
  await window("main");
  await click("STOP");
  await phase("off");
  assert.equal(fs.existsSync(ownerFile), false);
  assert.equal((await evidence()).present, false);
  await ipc("refresh", { models: false });
  await phase("off");
  await noCloud();
}
async function reset(boot) {
  await control({ action: "reset", boot });
  await window("main");
  await ipc("refresh", { models: false });
  await phase("ready");
}
async function holdRepair() {
  await clients();
  const before = (await evidence()).hold_entered;
  await control({ action: "hold" });
  const sequence = nativeTrace().at(-1)?.seq ?? -1;
  await click("Configure/Repair");
  await browser.waitUntil(async () => (await evidence()).hold_entered > before,
    { timeout: 4000, timeoutMsg: "authenticated model discovery did not enter its hold" });
  return sequence;
}
async function deniedSetupAfter(sequence) {
  await browser.waitUntil(() => nativeTrace().some((entry) => entry.seq > sequence
    && entry.event === "configure_opencode_completed" && entry.result === "denied"),
    { timeoutMsg: "held real Configure/Repair did not reject" });
}
async function nativeDrag(label) {
  await window(label);
  const before = await browser.getWindowRect();
  fs.writeFileSync(path.join(root, `drag-${label}-ready.json`), JSON.stringify(before));
  await browser.waitUntil(async () => {
    const after = await browser.getWindowRect();
    fs.writeFileSync(path.join(root, `drag-${label}-sample.json`), JSON.stringify({ before, after }));
    if (Math.abs(after.x - before.x) + Math.abs(after.y - before.y) < 40) return false;
    acceptance[`${label}_drag`] = "native input moved the window";
    writeAcceptance();
    return true;
  }, { timeout: 180000, timeoutMsg: `native title-bar drag did not move ${label}` });
  await fits();
}

describe("native runtime UI with owned providers (no inference)", () => {
  before(() => writeAcceptance());

  it("shows owned loading then Ready without Start", async () => {
    await window("main");
    await phase("booting");
    await control({ action: "stage", stage: "ready" });
    await ipc("refresh", { models: false });
    await phase("ready");
    const state = await ipc("get_state");
    assert.equal(state.target, "local");
    assert.equal(state.snap.status.model, "q6");
    assert.equal(state.snap.status.ctx, 8192);
    assert.equal(state.endpoint, fixture.api_url);
    await noCloud();
  });

  it("opens Clients from Ready and configures the verified running model", async () => {
    await clients();
    const checkbox = $("#section-clients input[type=checkbox]");
    assert.equal(await checkbox.isSelected(), true);
    assert.match(await $("#section-clients").getText(), /8,192 context tokens/);
    await repair();
    await browser.waitUntil(async () => (await $(".clients .messages").getText()).includes("Restart OpenCode"));
    const source = fs.readFileSync(config, "utf8");
    assert.match(source, /Native fixture comment must survive/);
    const saved = JSON.parse(source.replace(/^\s*\/\/.*$/gm, ""));
    assert.equal(saved.provider.other.name, "preserved");
    assert.equal(saved.provider["lobo-local"].options.baseURL, fixture.api_url);
    assert.match(saved.provider["lobo-local"].options.apiKey, /^\{file:/);
    assert.equal(saved.model, `lobo-local/${alias}`);
    assert.equal(saved.default_agent, "lobo");
    assert.equal(saved.agent.lobo.temperature, 0.2);
    assert.deepEqual(saved.agent.lobo.permission, { edit: "ask" });
    assert.equal(saved.agent.lobo.model, `lobo-local/${alias}`);
    for (const file of inventory(path.join(root, "opencode-keys"))) assert.equal(file.mode, 0o600);
    assert.equal(fs.statSync(path.join(root, "opencode-keys")).mode & 0o777, 0o700);
    await fits();
    await noCloud();
  });

  it("repairs unchanged bytes without adding private or backup files", async () => {
    const before = privateFiles();
    await repair();
    unchanged(before);
    const result = await ipc("configure_opencode", { path: config, makeDefault: true });
    assert.equal(result.changed, false);
    unchanged(before);
    assert.match(await $(".clients .messages").getText(), /Restart OpenCode.*Project settings can override/s);
  });

  it("preserves top-level defaults when the visible checkbox is unchecked", async () => {
    const source = fs.readFileSync(config, "utf8");
    fs.writeFileSync(config, source.replace(`"lobo-local/${alias}"`, '"other/saved-default"')
      .replace('"default_agent": "lobo"', '"default_agent": "existing"'));
    await $("#section-clients input[type=checkbox]").click();
    assert.equal(await $("#section-clients input[type=checkbox]").isSelected(), false);
    await repair();
    const saved = JSON.parse(fs.readFileSync(config, "utf8").replace(/^\s*\/\/.*$/gm, ""));
    assert.equal(saved.model, "other/saved-default");
    assert.equal(saved.default_agent, "existing");
    assert.equal(saved.agent.lobo.model, `lobo-local/${alias}`);
  });

  it("rejects wrong authentication without changing config or private files", async () => {
    const before = privateFiles();
    await control({ action: "auth", reject: true });
    try {
      await repair();
      await browser.waitUntil(async () => (await $(".clients .messages").getText()).includes("rejected the saved API key"));
      unchanged(before);
    } finally { await control({ action: "auth", reject: false }); }
    await noCloud();
  });

  it("rejects held setup after a changed boot", async () => {
    const before = privateFiles();
    const sequence = await holdRepair();
    try {
      await control({ action: "replace_boot" });
      await control({ action: "release" });
      await deniedSetupAfter(sequence);
      await window("settings");
      unchanged(before);
    } finally {
      await control({ action: "release" });
      await control({ action: "restore_boot" });
    }
    await window("main");
    await ipc("refresh", { models: false });
    await phase("ready");
  });

  it("Local Stop reaches Off with zero cloud calls and rejects stale setup", async () => {
    const before = privateFiles();
    const sequence = await holdRepair();
    try {
      await stop();
    } finally { await control({ action: "release" }); }
    await deniedSetupAfter(sequence);
    await window("settings");
    await browser.waitUntil(async () => await $("button*=Configure/Repair").isExisting());
    unchanged(before);
    assert.equal(await $("button*=Configure/Repair").isEnabled(), false);
    await noCloud();
  });

  it("denies Start before runtime construction and generation", async () => {
    await window("main");
    const error = await browser.execute(async () => {
      try { await window.__TAURI_INTERNALS__.invoke("start"); return null; }
      catch (error) { return { kind: error.kind, message: error.message }; }
    });
    assert.ok(error);
    assert.equal(error.message, "Runtime UI fixture cannot start a runtime.");
    for (const relative of ["state/lobo/local.json", "weights/runtime", "weights/.e2e-llama.pid"])
      assert.equal(fs.existsSync(path.join(root, relative)), false);
    await noCloud();
    await ipc("dismiss");
    await reset("fixture-boot-2");
  });

  it("fits each Settings tab and reopens the same native process", async () => {
    await clients();
    for (const label of ["Local", "Cloud", "Defaults", "Clients"]) {
      await click(label, `button[role=tab]*=${label}`);
      await fits();
    }
    if (process.env.LOBO_E2E_CHECK_DRAG === "1") await nativeDrag("settings");
    await browser.closeWindow();
    await window("main");
    await visible("settings", false);
    if (process.env.LOBO_E2E_CHECK_DRAG === "1") await nativeDrag("main");
    const traces = fs.readFileSync(path.join(root, "driver-trace.jsonl"), "utf8").trim().split("\n").map(JSON.parse);
    const child = traces.find((entry) => entry.event === "native_child_observed");
    assert.ok(child?.pid);
    await browser.closeWindow();
    // Handles and DOM can survive close-to-hide. Check native visibility itself.
    await visible("main", false);
    process.kill(child.pid, 0); // Read-only existence check for this service-owned child.
    const bundle = path.resolve("../src-tauri/target/debug/bundle/macos/lobocode E2E.app");
    execFileSync("open", [bundle]);
    await window("main");
    await ipc("refresh", { models: false });
    await phase("ready");
    const after = fs.readFileSync(path.join(root, "driver-trace.jsonl"), "utf8");
    assert.ok(!after.includes('"event":"native_child_exit"'), "native process exited before reopen");
    acceptance.close_reopen = "same child remains alive; main visible after LaunchServices reopen";
    writeAcceptance();
    await stop();
  });
});
