import fs from "node:fs";
import path from "node:path";
import Worker, { launcher as TauriLauncher } from "@wdio/tauri-service";

export default Worker;
export class launcher extends TauriLauncher {
  observed = new WeakSet();
  cleanupExpected = false;

  trace(event, data = {}) {
    fs.appendFileSync(path.join(process.env.LOBO_E2E_ROOT, "driver-trace.jsonl"),
      JSON.stringify({ at: Date.now(), event, ...data }) + "\n");
  }

  observe() {
    // Pinned @wdio/tauri-service 1.4.0 stores the actual native app child here.
    // Fail visibly if an upgrade changes the map; never search other processes.
    if (!(this.embeddedProcesses instanceof Map)) {
      this.trace("unsupported_child_map");
      throw new Error("Pinned Tauri launcher child map changed; exit tracing unavailable");
    }
    for (const info of this.embeddedProcesses.values()) {
      const proc = info?.proc;
      if (!proc || typeof proc.once !== "function" || !Array.isArray(info.logHandlers)) {
        this.trace("unsupported_child_record");
        throw new Error("Pinned Tauri launcher child record changed; exit tracing unavailable");
      }
      if (this.observed.has(proc)) continue;
      this.observed.add(proc);
      this.trace("native_child_observed", { pid: proc.pid ?? null });
      const exited = (code, signal) => this.trace("native_child_exit", {
        pid: proc.pid ?? null, code, signal, expectedCleanup: this.cleanupExpected,
      });
      if (proc.exitCode !== null || proc.signalCode !== null) exited(proc.exitCode, proc.signalCode);
      else proc.once("exit", exited);
    }
  }

  async onPrepare(...args) {
    try { await super.onPrepare(...args); }
    finally { this.observe(); }
  }

  async onWorkerStart(...args) {
    try { await super.onWorkerStart(...args); }
    finally { this.observe(); }
  }

  async onComplete(...args) {
    this.observe();
    this.cleanupExpected = true;
    this.trace("driver_cleanup_expected");
    return super.onComplete(...args);
  }
}
