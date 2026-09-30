# Rust rewrite P5 — Tauri 2 app Implementation Plan v1.2

**Date:** 2026-09-29
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.
**Spec:** ./spec.md (full-auto implementation authorized, 2026-09-29)
**Contracts:** ./contracts.md v1.2 (this plan's "Contract additions" are folded in there)
**Phase:** P5 of 6. Starts after P4 is done on `feat/rust`.

**Goal:** `app/` replaces `macos/`. Same menu bar item, panel, Settings window, notifications and renders as the Swift app at `6a72092` (branch `fix/app-silent`, incl. panel-on-launch/reopen). It links `lobo-core` directly. No CLI subprocess, no JSON parsing of CLI output.

**Architecture:** The Swift `Store` becomes a Rust controller inside the Tauri process (`app/src-tauri`). A pure reducer (`store.rs`) holds every state rule and is unit-tested without Tauri. An async controller (`controller.rs`) runs the poll loop, the `up` stream, the stop sequence and notifications against a `Backend` trait (real = `lobo-core`, test = fake). Every state change emits one `PanelState` on event `lobo://state` and updates the tray. The Svelte UI renders `PanelState` and sends actions as Tauri commands. It computes nothing that the tray or a notification also needs. The local supervisor runs as the same binary re-exec'd with `--lobo-local-run`.

**Tech Stack:** Rust 1.98.1 (edition 2024), tauri 2, tauri-build 2, tauri-plugin-positioner (feature `tray-icon`), tauri-plugin-notification, tauri-plugin-clipboard-manager, tauri-plugin-opener, tauri-plugin-dialog, tiny-skia (tray + app icon drawing), ts-rs. UI: TypeScript, Svelte 5, vite, vitest, svelte-check, pnpm. Versions pinned in Task 2.

> Execution: the primary agent implements task-by-task and records checked work. No unavailable skill or model is required.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

Extra rule for P5 implementers: never launch the built app, never run `cargo tauri dev`, never start `lobo-core` providers for real. The orchestrator does all app runs (Task 40).

---

## Decisions (scored for the goal "parity, no drift, works behind the notch")

**D1. Where the Store logic lives**
- **A. Rust controller + pure reducer, TS renders `PanelState` — 9/10.** Tray title, tray icon and notifications need phase, step and progress while no window is open. A hidden WKWebView gets its timers throttled, so a TS state machine would stall the tray. Loses 1: `now`-based countdowns (kill-in, uptime) are formatted in TS and in Rust; a shared case file (Task 12) guards the pair.
- **B. TS store, Rust only thin commands (contract v1.0 `up` streaming to TS) — 4/10.** Tray and notifications break when no webview is visible. Port rules and readiness would be re-typed in TS.
- **C. Split per feature (boot log in TS, poll in Rust) — 3/10.** Two owners of `phase`. The Swift bugs came from exactly this.
- Pick A. Rule for the split: anything that decides state, readiness, ports, providers, defaults, validation, menu text or notification text is Rust (`lobo-core` when the CLI shares it, else `app/src-tauri/src/store.rs`). TS maps `PanelState` to strings, colours and layout only.

**D2. Panel surfaces**
- **A. Two windows: `panel` (tray popover: no decorations, hides on blur, TrayCenter) and `main` (titled window, opened on launch/reopen) — 8/10.** Same Svelte view in both. Matches Swift (MenuBarExtra + PanelWindow). Loses 2: a second webview (~30 MB, unmeasured) while `main` is open.
- **B. One window, toggle decorations at runtime — 5/10.** `set_decorations` with an overlay titlebar on macOS flickers and loses the transparent titlebar (guess, not tested). Saves one webview.
- **C. Only a titled window, no popover — 3/10.** Loses the tray panel, which is the product.

**D3. Tray text**
- **A. `TrayIcon::set_title` next to a 16 pt non-template icon — 8/10.** Verified on docs.rs (tauri 2.7 `TrayIcon::set_title`: macOS supported, Windows unsupported, Linux only with an icon). Loses 2: the title uses the system font, not SF Mono with monospaced digits like Swift (`StatusItem.swift:11`). Unverified how much it jumps; checked in Task 40.
- **B. Draw icon + text into one image — 5/10.** Monospaced, but the image width changes per word, template mode is lost for the whole item, and text rendering needs a font dependency.
- Pick A. Fallback if the user rejects the width jumps in Task 40: B as a new task. It needs a font rasteriser (`fontdue`) and a bundled monospaced font; SF Mono can't be bundled (licence), so the menu bar text would not match the system font either.

**D4. dmg**
- **A. `cargo tauri build --bundles app`, then today's `hdiutil` step — 8/10.** Same image as `fix/app-silent` (`Makefile:61-69`), proven on `macos-15`. Loses 2: one more Makefile block than the bundler.
- **B. `cargo tauri build --bundles dmg` — 6/10.** Tauri's `bundle_dmg.sh` drives Finder via AppleScript to lay out the window; on headless CI that is a known fragile step (not measured here). Also names the file `lobocode_<ver>_aarch64.dmg`, so release.yml changes.

**D5. Where `macos/` is removed:** here, in P5. Spec "File-level changes": "Removed on the branch … in the phase that replaces each part." P5 replaces the app. Removal is the last code task (Task 38), after the user approved the renders, and after the Swift baseline renders were taken (Task 0).

---

## File map

**Create**
- `app/src-tauri/Cargo.toml` — package `lobocode-app`, `[[bin]] name = "lobocode"`. Its OWN workspace (empty `[workspace]` table), NOT a member of the root workspace: path deps `lobo-core = { path = "../../crates/lobo-core" }`, `lobo-proto = { path = "../../crates/lobo-proto" }`; own `Cargo.lock`; `[lints]` copied from the root table. Why: the pod Dockerfile (P2) copies only `crates/`, and ubuntu CI has no webkit2gtk. Both keep working untouched (Codex finding 3).
- `app/src-tauri/build.rs` — `tauri_build::build()`.
- `app/src-tauri/tauri.conf.json` — productName `lobocode`, mainBinaryName `lobocode`, identifier `io.github.1905.lobocode`, `bundle.macOS.minimumSystemVersion` `13.0`, `bundle.macOS.signingIdentity` `-` (ad-hoc), `bundle.icon` from `icons/`, `build.frontendDist` `../ui/dist`, `build.devUrl` `http://localhost:5173`, `app.windows` `[]` (all windows are created in Rust).
- `app/src-tauri/Info.plist` — `LSUIElement` true (merged into the bundle's plist by tauri-bundler).
- `app/src-tauri/capabilities/default.json` — windows `panel`, `main`, `settings`: `core:default`, `core:window:allow-set-size`, `core:event:default`. Nothing else (clipboard, opener, dialog are Rust-side only).
- `app/src-tauri/icons/*` — generated by `cargo tauri icon` (Task 25). Committed.
- `app/src-tauri/src/main.rs` — argv switch: `--lobo-local-run` → supervisor, else Tauri app.
- `app/src-tauri/src/lib.rs` — `pub fn run()`, module list.
- `app/src-tauri/src/types.rs` — `Phase`, `Target`, `Step`, `StepMark`, `PanelState`, `AppError` (+ ts-rs).
- `app/src-tauri/src/store.rs` — pure reducer (port of `Store.swift` + `StatusItem.swift` extension).
- `app/src-tauri/src/fmt.rs` — `duration`, used in notification text.
- `app/src-tauri/src/backend.rs` — `trait Backend`, `CoreBackend`, `#[cfg(test)] FakeBackend`.
- `app/src-tauri/src/controller.rs` — poll loop, start, stop, dismiss, notifications.
- `app/src-tauri/src/notify.rs` — `trait Notifier`, Tauri impl.
- `app/src-tauri/src/prefs.rs` — saved target (`prefs.json` in app config dir).
- `app/src-tauri/src/commands.rs` — every `#[tauri::command]`.
- `app/src-tauri/src/tray.rs` — tray build + update.
- `app/src-tauri/src/icons.rs` — tiny-skia drawing: tray icon per phase, 1024 px app icon.
- `app/src-tauri/src/windows.rs` — create/show `panel`, `main`, `settings`.
- `app/src-tauri/src/supervisor.rs` — `--lobo-local-run` entry.
- `app/src-tauri/examples/render_icons.rs` — writes tray PNGs + `icon_1024.png` to a dir.
- `app/src-tauri/tests/fixtures.rs` — writes `app/ui/src/fixtures/panel_*.json` + `settings.json` from the reducer (like ts-rs export).
- `app/ui/package.json`, `pnpm-lock.yaml`, `vite.config.ts`, `tsconfig.json`, `svelte.config.js`, `index.html`.
- `app/ui/src/main.ts` — mounts by `?view=panel|settings|render`.
- `app/ui/src/lib/theme.css` — tokens (Task 26).
- `app/ui/src/lib/fmt.ts`, `fmt.test.ts`.
- `app/ui/src/lib/view.ts`, `view.test.ts` — `PanelState` → view models.
- `app/ui/src/lib/settings.ts`, `settings.test.ts` — form fields + diff.
- `app/ui/src/lib/api.ts` — typed `invoke` wrappers + `onState`.
- `app/ui/src/widgets/{Logo,RasterBar,Scanlines,GlitchText,Cursor,BracketButton,LinkButton,BracketPicker}.svelte`.
- `app/ui/src/panel/{Panel,Header,Footer,SetupCard,StartCard,LocalStart,BootLog,ReadyCard,FailCard}.svelte`.
- `app/ui/src/settings/Settings.svelte`.
- `app/ui/src/render/Render.svelte` — every fixture state, one per URL.
- `app/ui/src/fixtures/*.json` — generated (Task 24) + `time_cases.json` (hand-written, Task 12).
- `app/ui/src/gen/*.ts` — ts-rs output of app types. Committed.
- `tools/render_review.py` — builds the review page (Swift baseline vs Tauri, per state).

**Modify**
- Root `Cargo.toml` — unchanged (the app is a separate workspace).
- `crates/lobo-proto/src/control.rs` — `UpRequest`. `crates/lobo-proto/src/config.rs` — `Readiness`. P1 `ts_exports_have_no_bigint` expected list gains both.
- `crates/lobo-core/src/config/mod.rs`, `src/control/precheck.rs`, `src/local/` (P3 layout) — contract additions (Tasks 3–7), only what P3 did not already ship.
- `Makefile` — `mac`, `dmg`, `install-mac` rebuilt; new `app-test`, `app-lint`, `app-render`, `app-icons`, `app-fixtures`.
- `.github/workflows/release.yml` — `dmg` job builds with Rust + pnpm + tauri-cli.
- `.github/workflows/rust.yml` — ubuntu jobs unchanged (app is its own workspace); new `app` job on `macos-15`.
- `README.md` — app section. `docs/img/{menubar_ready,panel_boot,panel_ready,settings}.png` — new renders.
- `.gitignore` — `app/ui/node_modules/`, `app/ui/dist/`.

**Remove (Task 38, `mv` to `/tmp/trash`, then `git add -A`)**
- `macos/` (all of it, incl. `make_icns.py`, `Info.plist`, `Tests/`).

**Out of scope for P5**
- Windows/Linux app builds. Code signing / notarization. Any new user-facing feature. Local performance.
- Changing panel look. This is a port: same strings, colours, sizes. The ux-design-guide redesign flow (variants, quiet pass) does not apply; its inventory, every-state renders, own-eyes check of each screenshot and review page do.

---

## Locked interfaces

### lobo-proto additions (Task 8)

```rust
/// What the app asks `up` for. None = config default (the CLI's "flag not set").
/// The app always sends provider and model (explicit Q8 must beat a saved LOBO_MODEL=q6, Store.swift:168).
pub struct UpRequest { pub provider: Option<String>, pub model: Option<String>, pub ctx: Option<i64>,
                       pub source: Option<String>, pub cloud: Option<String> }
/// Config readiness, computed by lobo-core from the file with the CLI's own rules.
pub struct Readiness { pub exists: bool, pub cloud_ready: bool, pub ready: bool, pub local_supported: bool,
                       pub providers: Vec<String>, pub default_provider: String, pub default_model: String,
                       pub local_port: u16, pub error: Option<String> }
```
Both follow the P1 wire rules (derives, `#[ts(export)]`, `i64` → `#[ts(type = "number")]`).

### lobo-core additions (Tasks 3–7; skip any P3 already shipped under the same name)

```rust
pub mod config {
    pub fn readiness(path: &Path) -> lobo_proto::Readiness;
      // exists = file exists. Load error → all false, error = Some(msg).
      // cloud_ready = exists && require_cloud() ok && LOBO_API_KEY set.
      // ready = cloud_ready || (local_supported && exists && LOBO_API_KEY set).
      // providers = Laptop::providers(); default_provider = Laptop::default_provider() (may be "local");
      // default_model = Defaults.model or DEFAULT_MODEL; local_port = Laptop::port().
    pub fn validate_set(set: &BTreeMap<String, String>) -> std::result::Result<(), String>;
      // SettingsView.swift:253-265 rules and exact messages; port rule via parse_local_port.
    // API keys: use P3's genkey::new_api_key() (the only generator). config::mask is P3's too (P3 Task 13).
}
pub mod control {
    pub fn resolve_up(cfg: &Laptop, cfg_path: &Path, req: &UpRequest, supported: fn() -> Result<()>) -> Result<UpOpts>;
      // app only. Thin composition of P3's apply_defaults + check_target (req field Some → flag "set").
      // The CLI keeps calling the P3 pieces directly. Same error texts.
    // v1.1: no deps_with_spawner. The app calls P3's deps_from_config(cfg, &Wiring::new(abs(config_path), Spawner::app(current_exe())))
}
pub mod local {
    pub fn free_bytes_nearest(path: &Path) -> Option<u64>;   // statfs of path, or of its nearest existing parent; "~/" expanded
    // RunConfig parsing = P3's RunConfig::from_args(args, version: Manifest); --boot-id optional (Go parity).
    // is_supervisor = P3's is_supervisor(pid, boot_id, ps): argv has "local" "run" as whole args AND
    // "--boot-id <boot_id>". SUPERVISOR_ARG alone does not match; Spawner::app puts "local run" in the argv.
}
```

### app crate (Tasks 9–23)

```rust
// types.rs  (ts-rs, #[ts(export, export_to = "../gen/")] so P1's proto/ file list stays lobo-proto only)
pub enum Phase { Loading, NoConfig, Off, Booting, Ready, Stopping, Failed { message: String } } // serde tag = "kind"
impl Phase { pub fn word(&self) -> &'static str; }            // SCAN SETUP OFF BOOT RUN STOP FAIL
pub enum Target { Local, Cloud }                              // lowercase
pub enum Step { Rent, Container, Tunnel, Gpu, Download, Load, Ready } // lowercase, this order
impl Step { pub fn steps(local: bool) -> &'static [Step]; pub fn label(self, local: bool) -> &'static str;
            pub fn from_up_phase(p: &str) -> Option<Step>; pub fn index(self) -> usize; }
pub struct StepMark { pub step: Step, pub at_s: f64 }         // seconds since boot_start
pub struct PanelState {
    pub phase: Phase, pub target: Target, pub provider: String, pub model: String,
    pub snap: Option<Snap>, pub config: Option<ConfigShow>, pub readiness: Option<Readiness>,
    pub models: Option<Listing>, pub catalog_ids: Vec<String>,   // DEFAULT_MODEL first, then the rest of catalog::all()
    pub download: Option<DownloadProgress>, pub steps: Vec<StepMark>, pub boot_start_ms: Option<i64>,
    pub up_phase: Option<String>, pub last_detail: String, pub warning: Option<String>, pub log_tail: Vec<String>,
    pub ready_url: Option<String>,
    // derived by the reducer, so TS never recomputes them:
    pub is_local: bool, pub boot_steps: Vec<Step>, pub current_step: Option<Step>, pub endpoint: Option<String>,
    pub menu_text: String, pub boot_progress: f64,
}
pub struct AppError { pub kind: String, pub message: String }  // from lobo_core::Error::kind() + Display

// store.rs — pure, no I/O, no clock (now passed in)
pub struct Store { /* PanelState fields + up_running, model_auto_picked, user_stopped, panel_open, saved_target */ }
impl Store {
    pub fn new(saved_target: Option<Target>, local_supported: bool) -> Store;
    pub fn view(&self, now: DateTime<Utc>) -> PanelState;
    pub fn poll_interval(&self) -> Duration;                      // Store.swift:121-127
    pub fn derive(s: &Snap, up_running: bool, current: &Phase) -> Phase;   // Store.swift:243-254
    pub fn default_target(saved: Option<Target>, r: Option<&Readiness>, cfg: Option<&ConfigShow>,
                          models: Option<&Listing>, local_supported: bool) -> Target;   // Store.swift:153-160
    pub fn apply_config(&mut self, c: ConfigShow, r: Readiness);               // Store.swift:176-189 (w/o I/O)
    pub fn apply_models(&mut self, m: Listing);                                // Store.swift:191-203
    pub fn apply_snap(&mut self, s: Snap, now: DateTime<Utc>) -> Vec<Note>;    // Store.swift:224-241
    pub fn begin_up(&mut self, now: DateTime<Utc>) -> UpRequest;              // Store.swift:256-266 + upArgs
    pub fn handle_event(&mut self, ev: &UpEvent, now: DateTime<Utc>) -> Vec<Note>;  // Store.swift:279-298
    pub fn up_ended(&mut self, last_err: Option<&str>) -> Vec<Note>;           // Store.swift:300-308
    pub fn begin_stop(&mut self) -> bool;                                       // returns is_local at press time
    pub fn stop_failed(&mut self, msg: String); pub fn stop_done(&mut self);
    pub fn dismiss(&mut self); pub fn choose(&mut self, t: Target); pub fn set_provider(&mut self, p: String);
    pub fn set_model(&mut self, m: String); pub fn set_warning(&mut self, w: Option<String>);
    pub fn poll_failed(&mut self, msg: String);                                 // S3: warning; loading → off
    pub fn set_panel_open(&mut self, open: bool);
}
pub struct Note { pub title: String, pub body: String }         // a notification to send

// fmt.rs
pub fn duration(secs: f64) -> String;                           // Store.swift:408-411

// backend.rs
#[async_trait] pub trait Backend: Send + Sync {
    fn config_path(&self) -> PathBuf;
    async fn config(&self) -> Result<(ConfigShow, Readiness), AppError>;
    async fn models(&self) -> Result<Listing, AppError>;          // local::list(Laptop::weights())
    async fn snapshot(&self) -> Result<Snap, AppError>;
    fn up(&self, req: UpRequest, cancel: CancellationToken) -> Result<lobo_core::control::UpOperation, AppError>;
    async fn down(&self) -> Result<f64, AppError>;
    async fn api_key(&self) -> Result<String, AppError>;          // clear value, only for the clipboard
    async fn save(&self, set: BTreeMap<String, String>) -> Result<(), AppError>;  // validate_set, then config::save
}
pub struct CoreBackend { pub path: PathBuf, pub wiring: control::Wiring }  // path = $LOBO_APP_CONFIG or config::default_path()

// controller.rs
pub struct Controller;   // Arc; holds Mutex<Store>, Arc<dyn Backend>, Arc<dyn Notifier>, Arc<dyn Clock>, emit: Arc<dyn Fn(PanelState)+Send+Sync>
impl Controller {
    pub fn new(b: Arc<dyn Backend>, n: Arc<dyn Notifier>, clock: Arc<dyn Clock>, prefs: Prefs,
               emit: Arc<dyn Fn(PanelState) + Send + Sync>) -> Arc<Controller>;
    pub fn spawn_loops(self: &Arc<Self>);                         // poll loop + 1 s tick (tray countdown)
    pub async fn refresh(&self, models: bool); pub async fn load_config(&self, models: bool);
    pub fn start(self: &Arc<Self>); pub fn stop(self: &Arc<Self>); pub fn dismiss(self: &Arc<Self>);
    pub fn choose(&self, t: Target); pub fn set_provider(&self, p: String); pub fn set_model(&self, m: String);
    pub fn panel_shown(self: &Arc<Self>, open: bool);            // open → refresh(models: true)
    pub fn state(&self) -> PanelState;
}
// Rule: never hold the Store mutex across an .await.

// notify.rs
pub trait Notifier: Send + Sync { fn send(&self, n: &Note); }

// prefs.rs
pub struct Prefs { pub target: Option<Target> }
impl Prefs { pub fn load(dir: &Path) -> Prefs; pub fn save(&self, dir: &Path) -> std::io::Result<()>; }  // <dir>/prefs.json

// icons.rs
pub fn tray_icon(phase: &Phase, progress: f64) -> (Vec<u8> /*RGBA*/, u32, u32, bool /*template*/); // 32×32 px = 16 pt @2x
pub fn app_icon_1024() -> tiny_skia::Pixmap;

// supervisor.rs
pub fn main(args: &[String]) -> i32;   // args after `SUPERVISOR_ARG local run` (args[4..]); RunConfig::from_args(.., Manifest) → tokio runtime → local::supervise; SIGINT/SIGTERM cancel
```

Tauri commands (`commands.rs`). TS calls only these:

| Command | Args → Result | Swift source |
|---|---|---|
| `get_state` | → `PanelState` | initial render |
| `start` / `stop` / `dismiss` | → `()` | `Store.start/stop/dismiss` |
| `choose_target` | `t: Target` | `Store.choose` |
| `set_provider` / `set_model` | `v: String` | pickers |
| `refresh` | `models: bool` | panel onAppear |
| `copy_api_key` | → `Result<(), AppError>` | `Store.copyAPIKey` (key never enters the webview) |
| `copy_text` | `s: String` | endpoint copy |
| `config_show` | → `Result<ConfigShow, AppError>` | contract |
| `config_save` | `set: BTreeMap<String,String>` → `Result<(), AppError>` | contract; runs `validate_set`, saves, then `load_config(models: true)` + `refresh(false)` |
| `local_models` | → `Result<Listing, AppError>` | contract |
| `catalog` | → `Vec<Model>` | contract |
| `free_bytes` | `path: String` → `Option<u64>` | `SettingsView.freeBytes` |
| `gen_api_key` | → `String` | `Mask.newAPIKey` |
| `choose_weights` | `start: String` → `Option<String>` | `SettingsView.chooseWeights` (dialog plugin, folder, can create) |
| `open_settings` / `reveal_config` / `open_config` / `quit` | → `()` | footer + file box |

Event: `lobo://state` → `PanelState`, emitted after every Store change and on the 1 s tick only when `menu_text` changed.

Contract v1.0 `snapshot`, `up` (+ `lobo://up`), `cancel_up`, `down` are not exposed to TS. The controller calls them through `Backend`. See "Contract additions".

---

## Pinned versions

_(Task 2 fills this line from `Cargo.lock` and `app/ui/pnpm-lock.yaml`: tauri, tauri-build, tauri-plugin-positioner, tauri-plugin-notification, tauri-plugin-clipboard-manager, tauri-plugin-opener, tauri-plugin-dialog, tiny-skia, async-trait, tauri-cli; svelte, vite, @sveltejs/vite-plugin-svelte, vitest, svelte-check, typescript, @tauri-apps/api, pnpm.)_

---

## Task 0 — Preconditions + Swift baseline renders (orchestrator)

- [ ] On `feat/rust`, tree clean. P4 status `done`. `git merge master` (merge-hygiene rule). ⚠ `6a72092` (`fix/app-silent`) must be on master and merged in; `grep -c applicationShouldHandleReopen macos/Sources/Lobocode/PanelWindow.swift` → `1`. If `0`: stop, the user merges `fix/app-silent` first.
- [ ] `cargo tauri --version` → `tauri-cli 2.x`. Measured 2026-09-29 on this Mac: `tauri-cli 1.5.11` → too old. Fix: `cargo install tauri-cli --version "^2" --locked`. `pnpm --version`, `node --version` present.
- [ ] Baseline renders from the Swift app (no CLI needed): `cd macos && swift build -c release && .build/release/Lobocode --render ../plans/2026-09-29-rust-rewrite/p5-baseline` → prints `rendered to …`. Expect 10 `panel_*.png`, 10 `menubar_*.png`, `settings.png`, `icon_1024.png`.
- [ ] `cd macos && swift test` → all pass (records the Swift test count for Task 41).
- [ ] Commit: `plans: rust P5 plan + Swift baseline renders`.

## Task 1 — Parity inventory = acceptance checklist (orchestrator)

The table below was built from the Swift source at `6a72092`. Orchestrator re-reads every cited line, fixes any wrong cite, and freezes it. Task 41 ticks every row. Ids are referenced by tasks as `[P#]`.

### Views and states

| Id | State / surface | What the user sees | Source |
|---|---|---|---|
| P1 | loading | `scanning providers` (dim) + blinking cursor | PanelView.swift:27-28 |
| P2 | setup (noConfig) | `no usable config yet` (amber bold 12); config path (dim 10, selectable; fallback `~/.config/lobo/config.env`); Apple Silicon: `needs an api key. cloud also needs a provider key, domain, tunnel token and bucket URL.` else `needs a provider key (RunPod or Vast), domain, tunnel token and bucket URL.`; `[ SETUP ]` amber wide → Settings | PanelView.swift:86-100 |
| P3 | off, target picker | `> target [local] cloud` only when local supported | PanelView.swift:108-111 |
| P4 | off cloud, no keys | row `> cloud  no keys` + `[ SETUP ]` amber wide | PanelView.swift:121-125 |
| P5 | off cloud, ready | provider picker if >1 provider else row `> provider  runpod`; model picker `q8 q6`; row `> limits  ≥100MB/s idle 30m max 12h` (11 pt; empty/0 → defaults 100/30/12); `[ START ]` green wide, Enter = default action | PanelView.swift:126-155 |
| P6 | off local, models loaded | one row per model: `[q6]` green / ` q8 ` dim, `22.1 GB`, state `✓ on disk` (text) / `partial 43%` (cyan, capped 99) / `↓ download` (dim); line `<weights> · 958.9 GB free` (10 pt dim, middle-truncated); click row = pick; `[ START ]` | PanelView.swift:159-201 |
| P7 | off local, models nil | model picker `q8 q6` + `[ START ]` | PanelView.swift:168-170 |
| P8 | booting | one row per boot step: `[ OK ]` green / `[ >> ]` cyan + cursor / `[ .. ]` faint; label (cloud: rent container tunnel gpu download load ready; local: start metal model load ready); step time `m:ss` dim; last detail 9 pt faint, 2 lines, middle-truncated; `T+m:ss` 11 bold cyan; `[ ABORT ]` red | PanelView.swift:203-247, Store.swift:33-61 |
| P9 | booting, download line | under the current download step, indent 56: bar 14 cells copper; `verify sha256` when `verifying`; else `12.4/28.6G` + `713MB/s` (local: text colour; cloud: green ≥100 else amber) + ETA `m:ss` dim if >0 | PanelView.swift:238-267 |
| P10 | booting, verify w/o download | `verify` (10 pt) when `upPhase == verify` or agent stage `verify` | PanelView.swift:241-243 |
| P11 | ready | `endpoint <url> copy`; `api key <masked> copy` (copies the clear key); `copy` → `copied` green for 1.2 s; tiles `gen`/`prompt` value 22 bold green glow + `tok/s` (≥100 → `%.0f`, else `%.1f`, nil `—`); | PanelView.swift:270-354 |
| P12 | ready, memory row | label `memory` (local) / `vram` (cloud), bar 12, `%.1f/%.1f GB` (MB/1024); cloud only `gpu 87%` (green if >0 else dim); row hidden when no GPU data | PanelView.swift:282-291 |
| P13 | ready, bottom row | `idle-stop ` (local) / `idle-kill ` + countdown (cloud <300 s amber); countdown frozen while `requests_processing > 0`, else `kill_in_s − (now − snap.at)`; `  ·  T+<uptime>`; right: local `local · $0`, cloud `$%.2f` spent = cost/h × elapsed; `[ STOP ]` red wide | PanelView.swift:292-322, Store.swift:371-375 |
| P14 | stopping | `[ .. ]` cyan + `stopping llama.cpp` (local) / `deleting pod on <provider>` + cursor | PanelView.swift:37-42 |
| P15 | failed | `[FAIL]` red bold + message; log box: last 5 of log tail, 9 pt faint, middle-truncated, card bg; buttons: pod present → `[ STOP <provider> ]` red, else `[ RETRY ]` green; `[ DISMISS ]` dim | PanelView.swift:356-386 |
| P16 | warning line | `! <poll error>` amber 10 pt, 2 lines, under any state | PanelView.swift:13-15 |
| P17 | header | block logo (gradient); raster bar (sweeps while boot/stop/loading); `sys:` + glitch word (SCAN SETUP OFF BOOT RUN STOP FAIL, phase colour); version right (9 pt faint, middle-truncated); detail line: pod → `provider · detail · $0.69/h` (cost only if >0); no pod + booting → `starting local` / `renting <provider>`; no pod + off → cloud `no pod · $0.00/h`, local empty | PanelView.swift:49-84, Store.swift:14-24 |
| P18 | footer | `settings ⌘,` (⌘,), `config` (reveal file in Finder), `quit ⌘q` (⌘q) | PanelView.swift:388-407 |
| P19 | panel frame | width 340, body padding 14, spacing 12, bg, dark | PanelView.swift:8-23 |
| P20 | menu bar item | icon + text: `lobo` loading, `setup`, `off`, `stop`, `FAIL`; booting: `42%` during download, else step label, else `boot`; ready: `45 t/s` if processing and gen_tps>0, else `27m` (ceil minutes left), else `run` | StatusItem.swift:28-48 |
| P21 | tray icon | 16×16, rounded frame (1.5,1.5,13,13) r3 stroke 1.5; ready = filled inner (inset 3, r1) `#2EE57A`; boot/stop/loading = fill from bottom, height = max(0.12, progress), α0.9, `#00C8E6`; failed = X stroke 1.5 `#FF4D5E`; setup = 3 px dot `#FFB020`; off = frame only, system secondary label colour; non-template | StatusItem.swift:51-99 |
| P22 | boot progress | step index / step count, download adds bytes/total share; ready = 1 | StatusItem.swift:17-26 |
| P23 | panel window | title `lobocode`, transparent titlebar, full-size content, dark, bg `#0B0D10`, centred, reused; shown at launch unless `--background`, and on every reopen (Finder/Spotlight/Launchpad) | PanelWindow.swift:1-43 |
| P24 | settings window | title `lobocode · config`, same style, 520×640, scrolls; opened by footer, SETUP buttons, `--settings` at launch (+0.5 s) | SettingsWindow.swift:1-26, LobocodeApp.swift:8-13, SettingsView.swift:23-31 |
| P25 | settings header | logo + `config` (10 dim) + raster bar active while saving | SettingsView.swift:34-40 |
| P26 | settings file box | path (11, selectable); `plain KEY=value lines, shared with the lobo CLI. edit it by hand any time: this window only touches the keys it shows.`; `reveal in finder`; `open in editor` (disabled when the file does not exist) | SettingsView.swift:84-99 |
| P27 | settings sections | `// providers  (one is enough)`: runpod key, vast key. `// access`: domain (`lobo.example.com`), api key, tunnel token, bucket url (`https://pub-….r2.dev`). `// defaults for lobo up  (empty = built-in)`: provider picker (targets = local if supported + keyed providers; only when >1; default runpod if present else first), model `q8 q6` (def q8), min MB/s `100`, context `65536`, idle min `30`, max hours `12`, runpod cloud `community secure` (def community), vast max $/h `1.20`, pod image `ghcr.io/1905/lobocode@sha256:…`. `// local` (Apple Silicon only): weights, port `8931` | SettingsView.swift:42-71 |
| P28 | secret field | placeholder `not set`, or `<masked>  (empty = keep, - = remove)`; typed value never shown back | SettingsView.swift:139-147 |
| P29 | api key row | masked current or `not set`; after `generate`: `<mask>  (new, unsaved)` amber | SettingsView.swift:149-157 |
| P30 | weights row | text field (placeholder = listing weights or `~/Library/Application Support/lobo/weights`), `[choose…]` folder dialog (can create, start at typed or current folder), `<n> GB free` under it: saved folder → listing's number; typed/chosen → measured on nearest existing parent | SettingsView.swift:159-213 |
| P31 | pickers | `[o]` green / ` o ` dim | SettingsView.swift:215-226, Theme.swift:195-215 |
| P32 | save | no LOBO_API_KEY and no new key → generate one; only changed keys (plain trimmed ≠ current; secret `-` = remove, empty = keep; new api key); none → `nothing changed` dim; validation error red; OK → `saved N key(s)` green, reload config + form, refresh; error → `save failed: <msg>` red; `[ REVERT ]` dim reloads form; `[ SAVE ]` green ⌘S, disabled while saving | SettingsView.swift:228-291 |
| P33 | validation texts | `<KEY>: whole number ≥ <min>, or empty` (MIN_MBPS 1, CTX 512, IDLE_MIN 1, MAX_HOURS 1; `0` allowed); `LOBO_VAST_MAX_DPH: a price like 1.20`; `LOBO_DOMAIN: bare hostname, no https://`; `LOBO_LOCAL_PORT: whole number 1024-65534, or empty` | SettingsView.swift:253-265 |
| P34 | app icon | 1024 canvas, 824 tile r185 continuous, bg fill, line stroke 10; `L` cyan + `C` magenta 5×7 pixel glyphs, cell 52, gap 40, cell inset 5% | Renderer.swift:139-153, PixelGlyph.swift:1-32 |

### Store behaviour

| Id | Behaviour | Source |
|---|---|---|
| S1 | Poll interval: booting/stopping/loading 3 s; ready 5 s panel open / 15 s closed; else 10 s / 30 s | Store.swift:121-127 |
| S2 | Start of app: load config + models once, then poll forever; 1 s clock tick; ask notification permission | Store.swift:102-119 |
| S3 | refresh: config missing or not ready → reload config (no models); still not ready → phase setup, stop. Status error → warning = message; loading → off. OK → warning cleared, apply; `models && down` → re-list models | Store.swift:207-221 |
| S4 | Panel open (either window) → `refresh(models: true)`; closed → panel_open false | LobocodeApp.swift:18-19, PanelWindow.swift:14-15 |
| S5 | derive: failed + down + no up → keep failed; stopping + !down → stopping; down → booting if up running else off; no agent status → booting; stage ready → ready; failed/terminating → booting if up running else failed(stage_detail or stage); else booting | Store.swift:243-254 |
| S6 | apply: download kept from status when stage download/verify; ready → off without user stop → notify `lobo stopped` / `stopped by itself (idle or expiry)`; off/ready clears user_stopped; booting with no boot_start → pod.started_at or now; resumed boot fills all steps ≤ agent stage with now − t0; off/ready with no up → clear boot_start, steps, download | Store.swift:224-241 |
| S7 | start: guard no up running; phase booting; boot_start now; clear steps, download, up_phase, log_tail, ready_url; last_detail `starting llama.cpp…` (local) / `renting <provider>…`; request = provider (`local` or picked), model explicit | Store.swift:256-277, 168-171 |
| S8 | up event: up_phase = phase; first time a step is seen → mark at now − t0; download replaced; detail → last_detail + log tail (keep 6); err → split lines into log tail (keep 8); ready → ready_url, phase ready, notify `lobo ready` / `<url> · <m:ss> · $0.69/h` (or `local`) | Store.swift:279-298 |
| S9 | up ended with error and phase not ready/stopping → failed(last non-empty log line, else error, else `exit`), notify `lobo boot failed` / msg; always refresh(models: true) | Store.swift:300-308 |
| S10 | Stop cancels and waits for UpOperation completion; 120 s warns but retains ownership. Down follows completion. Cleanup failure stays visible. | v1.2 deliberate cleanup fix |
| S11 | dismiss → off, refresh | Store.swift:340-343 |
| S12 | load config: set config; if no up and not booting → provider = default provider, model = default model, auto-pick reset; apply default target | Store.swift:176-189 |
| S13 | load models (local supported only): once per config load, no LOBO_MODEL set, picked model not on disk, another is → pick that one; apply default target | Store.swift:191-203 |
| S14 | default target: not supported → cloud; saved pick; LOBO_PROVIDER=local → local; no provider keys and a model on disk → local; else cloud. Applied only when no up, not booting, not ready | Store.swift:153-166 |
| S15 | target pick saved (`lobo.target` in UserDefaults), previews never read it | Store.swift:99, 144-150 |
| S16 | is_local: live pod provider == local, else target == local | Store.swift:135-138 |
| S17 | endpoint: ready_url; else local → `http://127.0.0.1:<port>/v1`; else `https://<LOBO_DOMAIN>/v1`; else none | Store.swift:354-359 |
| S18 | local port: LOBO_LOCAL_PORT in 1024-65534, else 8931 | Store.swift:361-369 |
| S19 | config readiness: providers = keyed runpod, vast; default provider = LOBO_PROVIDER if keyed else first else runpod; default model q6 only if LOBO_MODEL=q6; cloud_ready = exists + a provider + DOMAIN, API_KEY, TUNNEL, BUCKET set; ready = cloud_ready or (Apple Silicon + exists + API_KEY) | Models.swift:97-124 |
| S20 | model state: size>0 and on_disk≥size → on disk; on_disk>0 → partial fraction; else missing | Models.swift:149-155 |
| S21 | notifications only inside a real .app bundle | Store.swift:379-393 |
| S22 | copy API key reads the clear key (not the masked one) | Store.swift:345-352 |
| S23 | LOBO_APP_CONFIG env = config path override (dev, tests) | CLI.swift:15-21 |
| S24 | `--render <dir>`, `--settings`, `--background` argv | main.swift:3-8, LobocodeApp.swift:9-12, PanelWindow.swift:32-35 |

### Theme tokens (Theme.swift)

| Id | Token | Value | Source |
|---|---|---|---|
| T1 | bg / card / line | `#0B0D10` / `#12161B` / `#1F252D` | Theme.swift:5-7 |
| T2 | text / dim / faint | `#D6DEE8` / `#6B7685` / `#3A424D` | Theme.swift:8-10 |
| T3 | green / cyan / magenta / amber / red | `#39FF88` / `#00E5FF` / `#FF2BD6` / `#FFB020` / `#FF4D5E` | Theme.swift:11-15 |
| T4 | copper gradient | left→right cyan, `#8A7BFF`, magenta | Theme.swift:16 |
| T5 | font | system monospaced; sizes 9, 10, 11, 12, 22; bold where noted | Theme.swift:18-20 |
| T6 | phase colours | ready green; boot/loading/stop cyan; failed red; setup amber; off dim | Theme.swift:22-30 |
| T7 | logo | 3-line block art, 9 pt bold, line spacing −1, gradient | Theme.swift:39-53 |
| T8 | raster bar | 2 px, gradient α0.35; active: white α0.9 slice 25% wide sweeps in 1.6 s; static with reduced motion | Theme.swift:55-78 |
| T9 | scanlines | 1 px white α0.03 every 3 px, header only | Theme.swift:80-92 |
| T10 | glitch text | on change: 9 frames × 30 ms from noise `!<>-_\/[]{}=+*^?#%&01`, spaces kept, settles left→right; off with reduced motion | Theme.swift:94-125 |
| T11 | cursor | `█` 10 pt cyan, blinks 0.5 s; solid with reduced motion | Theme.swift:127-137 |
| T12 | bracket button | `[ LABEL ]` 12 bold; padding 7/10; radius 4; bg colour α0.06 (pressed 0.25); border α0.7; hover: fill colour, text bg, glow r8 α0.45, 120 ms ease-out; disabled faint | Theme.swift:139-173 |
| T13 | link button | 10 pt, colour (default dim), hover → text | Theme.swift:175-193 |
| T14 | picker | `> label` dim, width 84; 12 pt | Theme.swift:195-215 |
| T15 | tile / box | radius 6, card fill, 1 px line stroke; field radius 4, padding 5/8; settings label width 110, form padding 22, spacing 18 | PanelView.swift:339-353, SettingsView.swift:101-119 |
| T16 | header box | padding 14/14/10, card + scanlines, 1 px line bottom; footer padding 14/9, card, 1 px line top | PanelView.swift:66-70, 401-405 |

### Formats

| Id | Format | Source |
|---|---|---|
| F1 | duration: `m:ss`, `h:mm:ss` from 1 h, rounded, never negative | Store.swift:408-411 |
| F2 | GB: bytes/1e9, 1 decimal | Store.swift:413 |
| F3 | bar: `▓`×round(f·w) + `░`, f clamped 0…1, NaN → 0 | Store.swift:415-420 |
| F4 | mask (app): <12 chars `••••`, else first 4 + `…` + last 4 | SettingsView.swift:294-296 |
| F5 | new API key: `sk-` + 48 hex (51 chars) | SettingsView.swift:297-301 |

- [ ] Re-check every cite against `git show 6a72092:<file>`. Fix wrong line numbers here.
- [ ] Commit: `plans: rust P5 parity inventory frozen`.

## Task 2 — Scaffold + pinned versions

**Files:** Create `app/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,Info.plist,capabilities/default.json,src/main.rs,src/lib.rs}`, `app/ui/{package.json,vite.config.ts,tsconfig.json,svelte.config.js,index.html,src/main.ts}`. Modify `Cargo.toml`, `.gitignore`.

- [ ] `cargo new --bin app/src-tauri --name lobocode-app --vcs none`; bin name `lobocode`; own workspace (`[workspace]` empty table), path deps to `../../crates/*`; `[lints]` copied from the root. Verify the pod image still builds: `docker build` is NOT run on the laptop; instead `cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; m=json.load(sys.stdin); assert not any("src-tauri" in p["manifest_path"] for p in m["packages"])'` from the repo root → exit 0 (the root workspace does not see the app).
- [ ] `cargo add --manifest-path app/src-tauri/Cargo.toml tauri --features tray-icon,image-png,macos-private-api`, `tauri-plugin-positioner --features tray-icon`, `tauri-plugin-notification`, `tauri-plugin-clipboard-manager`, `tauri-plugin-opener`, `tauri-plugin-dialog`, `tiny-skia`, `async-trait`, `tokio --features rt-multi-thread,macros,signal,time,sync`, `tokio-util`, `serde --features derive`, `serde_json`, `chrono`, `ts-rs`, `thiserror`, path deps `lobo-proto`, `lobo-core`. `cargo add --manifest-path app/src-tauri/Cargo.toml --build tauri-build`. All `cargo add` here run with `--manifest-path app/src-tauri/Cargo.toml`. Versions live in the app's own manifest; keep serde/chrono/tokio at the root workspace's versions (one line check: `cargo tree --manifest-path app/src-tauri/Cargo.toml -d` shows no duplicate serde/tokio).
- [ ] `pnpm create vite app/ui --template svelte-ts` equivalent by hand; `pnpm -C app/ui add @tauri-apps/api@^2`; `pnpm -C app/ui add -D svelte@^5 vite @sveltejs/vite-plugin-svelte vitest svelte-check typescript`. Scripts: `dev`, `build` (`vite build`), `test` (`vitest run`), `check` (`svelte-check --fail-on-warnings`).
- [ ] `vite.config.ts`: `server.port 5173`, `strictPort`, `server.fs.allow` includes `../../crates/lobo-proto/fixtures` (vitest reads P1 fixtures).
- [ ] `tauri.conf.json` as in the File map. `Info.plist`: `LSUIElement` true only.
- [ ] `main.rs` / `lib.rs`: `lib::run()` builds an empty Tauri app with the five plugins registered. Nothing else yet.
- [ ] Verify: `pnpm -C app/ui install && pnpm -C app/ui build` → `dist/index.html`. `cargo build --manifest-path app/src-tauri/Cargo.toml` → exit 0. `cargo clippy --manifest-path app/src-tauri/Cargo.toml -- -D warnings` → clean.
- [ ] Fill "Pinned versions" from the lockfiles. Record `cargo tauri --version` there.
- [ ] Commit: `app: tauri 2 + svelte 5 scaffold`.

## Task 3 — lobo-core: `config::readiness` [S19]

**Files:** Modify `crates/lobo-core/src/config/mod.rs` (+ tests). Needs `Readiness` from Task 8: run Task 8 before Task 3 (numbering groups by crate, not order).

- [ ] Failing tests (port `testReadyNeedsAPIKey` + `ConfigShow` provider tests, LobocodeTests.swift:240-251), temp config files: 4 cloud keys + runpod key, no API key → `ready false`; + API key → `cloud_ready true, ready true`; only API key → `cloud_ready false`, `ready == local_supported`; missing file → `exists false, ready false`; `LOBO_PROVIDER=vast` with only runpod key → `default_provider "runpod"`; `LOBO_PROVIDER=local` → `"local"`; `LOBO_MODEL=q6` → `default_model "q6"`; `LOBO_LOCAL_PORT=80` → `local_port 8931`; bad bucket URL → `cloud_ready false` (CLI rule, see Spec issues #5); unparsable file → `error Some`.
- [ ] Implement per the locked doc comment. `cargo test -p lobo-core readiness` → pass.
- [ ] Commit: `lobo-core: config readiness for the app (CLI rules)`.

## Task 4 — lobo-core: `validate_set` [P33, F4, F5]

**Files:** Modify `crates/lobo-core/src/config/mod.rs`. `genkey::new_api_key` and `config::mask` are already shipped by P3; the rows below only test them.

- [ ] Failing tests (port LobocodeTests.swift:211-238, 253-259): `LOBO_CTX=100` → Err; `LOBO_CTX=0` → Ok; `LOBO_DOMAIN=https://x` → Err `LOBO_DOMAIN: bare hostname, no https://`; `LOBO_LOCAL_PORT=80` → Err exactly `LOBO_LOCAL_PORT: whole number 1024-65534, or empty`; `""`, `1024`, `8931`, `65534` → Ok; `1023 65535 0 -1 89.31 port " 8931"` → Err each (note: `0` is Err here although `parse_local_port("0")` is Ok — Swift behaviour, keep it); `LOBO_VAST_MAX_DPH=0` → Err; `genkey::new_api_key()` len 51, prefix `sk-`, hex; `config::mask`: `mask("")` `(not set)`, `mask("short")` `••••`, `mask("sk-0123456789abcdef")` `sk-0…cdef`.
- [ ] Implement `validate_set` only. `cargo test -p lobo-core validate_set && cargo test -p lobo-core new_api_key && cargo test -p lobo-core mask` → pass.
- [ ] Commit: `lobo-core: settings validation`.

## Task 5 — lobo-core: `control::resolve_up`

**Files:** Modify `crates/lobo-core/src/control/precheck.rs`. `resolve_up` only composes P3's `apply_defaults` + `check_target`. The defaults table stays in P3 Task 62; nothing moves out of lobo-cli.

- [ ] Failing tests (the composition only: req field → `set`, cloud default, `supported` seam; the full defaults table is P3 Task 62): empty request → provider `default_provider()`, model/ctx/idle/max-life/min-mbps/cloud from `Defaults`; request `model Some("q8")` with `LOBO_MODEL=q6` → `q8`; provider `local` on a non-arm64 host → Err `local mode needs macOS on Apple Silicon…` (use the injected `supported` seam P3 uses); provider `vast` without key → Err `no VASTAI_API_KEY in <path>. Run \`lobo config\` to add it`; provider `x` → Err `--provider: want runpod, vast or local, got "x"`; cloud provider with missing tunnel → Err `config: cloud needs CF_TUNNEL_TOKEN`; bad `LOBO_CTX` in file with `ctx Some(4096)` → Ok (flag overrides a bad key).
- [ ] Implement. `cargo test -p lobo-core resolve_up` → pass.
- [ ] Commit: `lobo-core: resolve_up for the app (composes apply_defaults + check_target)`.

## Task 6 — lobo-core: spawner wiring + supervisor identity

**Files:** Modify `crates/lobo-core/src/control/` (wiring tests), `crates/lobo-core/src/local/*.rs`.

- [ ] Failing tests: `is_supervisor(pid, boot_id, &command_of)` on a helper child started as `<exe> --lobo-local-run local run --boot-id B1 --model q6` → true for `B1`, false for `B2`; `<exe> --lobo-local-run --boot-id B1` → false (no `local run`); `<exe> local run --boot-id B1` → true (unchanged); `<exe> --lobo-local-run-x --boot-id B1` → false (whole arg). `deps_from_config(cfg, &Wiring::new("/cfg/override.env", Spawner::app("/x/lobocode")))` → the local provider's spawn argv starts with `/x/lobocode --lobo-local-run local run` and carries `--config /cfg/override.env` (assert through the provider's argv builder, no process started). Second row: default path ≠ override path → the argv carries the override, never the default (Codex finding 2).
- [ ] Implement. `cargo test -p lobo-core supervisor && cargo test -p lobo-core spawner` → pass.
- [ ] Commit: `lobo-core: app spawner and --lobo-local-run supervisor identity`.

## Task 7 — lobo-core: `RunConfig::from_args` rows, `free_bytes_nearest`

**Files:** Modify `crates/lobo-core/src/local/*.rs`.

- [ ] Failing tests: `RunConfig::from_args(["--model","q6","--ctx","65536","--idle-min","30","--boot-id","b","--port","8931","--api-port","8932"], Manifest::default())` → matching fields; `--config /p` accepted before the flags; unknown flag → Err; missing `--boot-id` → Ok (optional, Go `local.go:77`). `free_bytes_nearest("/tmp/lobocode-no-such-dir/weights")` → `Some(_)` (port LobocodeTests.swift:237); `"~/"` expands.
- [ ] Implement `free_bytes_nearest`. `RunConfig::from_args` is P3's; add only the rows above if P3 lacks them.
- [ ] `cargo test -p lobo-core run_config && cargo test -p lobo-core free_bytes` → pass.
- [ ] Commit: `lobo-core: RunConfig::from_args rows for the app re-exec, free_bytes_nearest`.

## Task 8 — lobo-proto: `UpRequest`, `Readiness`

**Files:** Modify `crates/lobo-proto/src/control.rs`, `crates/lobo-proto/src/config.rs`, `crates/lobo-proto/src/lib.rs` (P1 expected TS list). Create `app/ui/src/proto/{UpRequest,Readiness}.ts` (generated).

- [ ] Failing tests: `UpRequest::default()` serialises all five fields as `null`; `Readiness` JSON round-trip of a hand-written value; P1 `ts_exports_have_no_bigint` expected list + `UpRequest`, `Readiness`.
- [ ] Implement. `make proto-ts && cargo test -p lobo-proto` → pass.
- [ ] Commit: `lobo-proto: UpRequest and Readiness for the app`.

## Task 9 — app types + TS export

**Files:** Create `app/src-tauri/src/types.rs`, `app/ui/src/gen/*.ts`. Modify `lib.rs`.

- [ ] Failing tests: `Phase::Failed{message:"x"}` serialises `{"kind":"failed","message":"x"}`; `Phase::word` table (7 rows, [P17]); `Step::steps(true)` labels `start metal model load ready`; `Step::steps(false)` = all 7; `from_up_phase`: `create→rent`, `image→container`, `boot→container`, `tunnel`, `gpu`, `download→download`, `verify→download`, `load`, `ready`, `warp→None` (port LobocodeTests.swift:87-96); `Step::Gpu.label(false)` = `gpu`.
- [ ] ts-rs with `export_to = "../gen/"`. Test `app_ts_exports`: `app/ui/src/gen/` holds exactly `Phase, Target, Step, StepMark, PanelState, AppError` and no `bigint`.
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml types && cargo test --manifest-path app/src-tauri/Cargo.toml app_ts_exports` → pass.
- [ ] Commit: `app: panel state types (ts-rs)`.

## Task 10 — reducer: derive + poll interval [S1, S5]

**Files:** Create `app/src-tauri/src/store.rs`.

- [ ] Failing tests: port `testDerive` (LobocodeTests.swift:47-57) row for row, using `Snap` built in Rust. `poll_interval` table: booting/stopping/loading → 3 s; ready open/closed → 5/15 s; off open/closed → 10/30 s.
- [ ] Implement. `cargo test --manifest-path app/src-tauri/Cargo.toml store::derive && cargo test --manifest-path app/src-tauri/Cargo.toml store::poll` → pass.
- [ ] Commit: `app: store derive + poll interval`.

## Task 11 — reducer: config, models, default target [S12–S16, S20]

- [ ] Failing tests: port `testDefaultTarget` (LobocodeTests.swift:139-153) row for row (fixture: P1 `listing.json` with q6 on disk; "empty" = same with `on_disk 0`). `apply_config` while booting keeps provider/model; while off sets them from `Readiness`. `apply_models` auto-pick: LOBO_MODEL empty, q8 picked, q8 partial, q6 on disk → q6; second `apply_models` without a new `apply_config` → no change; `LOBO_MODEL=q8` set → never auto-picks. `choose(Local)` sets target and marks it saved. `is_local`: pod provider `local` wins over target cloud.
- [ ] Implement. `cargo test --manifest-path app/src-tauri/Cargo.toml store::target && cargo test --manifest-path app/src-tauri/Cargo.toml store::models` → pass.
- [ ] Commit: `app: store config/models/target rules`.

## Task 12 — reducer: menu text, progress, endpoint + shared time cases [P20, P22, S17, S18, F1]

**Files:** Modify `store.rs`. Create `app/src-tauri/src/fmt.rs`, `app/ui/src/fixtures/time_cases.json`.

`time_cases.json` (hand-written; Rust and vitest both read it):
```json
{ "duration": [[0,"0:00"],[72,"1:12"],[8342,"2:19:02"],[-5,"0:00"],[59.6,"1:00"]],
  "kill_left": [{"kill_in_s":1634,"since_snap_s":34,"processing":0,"left_s":1600,"menu":"27m"},
                {"kill_in_s":1634,"since_snap_s":34,"processing":1,"left_s":1634,"menu":"28m"},
                {"kill_in_s":10,"since_snap_s":40,"processing":0,"left_s":0,"menu":"0m"}] }
```
- [ ] Failing tests: `fmt::duration` over `time_cases.duration`. Port `testUpEventsDriveSteps` + `testLocalBoot` menu/progress asserts (LobocodeTests.swift:59-72, 98-125): download 50/100 → `menu_text "50%"`, `boot_progress (4+0.5)/7`; local after `gpu` → `metal`, `0.2`. Ready: processing 1 + gen_tps 45.1 → `45 t/s`; not processing → `kill_left.menu` column (note row 2: processing but gen_tps 0 → minutes, frozen). Menu words for loading/setup/off/stop/failed. Endpoint: when a local instance is running, `endpoint` = that instance's `api_url` (from the running state's ports, carry-over rule; `provider.go:293-297`); `Readiness.local_port` only when nothing runs. Tests: port `testLocalEndpointWithoutReadyEvent` (LobocodeTests.swift:127-137) with no instance → port 9000 from config; new `endpoint_prefers_running_state`: instance on 8931, config says 9000, no ready event (app reopened) → endpoint uses 8931 (Codex finding 9; deliberate change from Swift).
- [ ] Implement `view(now)` filling the derived fields. `cargo test --manifest-path app/src-tauri/Cargo.toml store::menu && cargo test --manifest-path app/src-tauri/Cargo.toml store::endpoint && cargo test --manifest-path app/src-tauri/Cargo.toml fmt` → pass.
- [ ] Commit: `app: menu text, boot progress, endpoint; shared time cases`.

## Task 13 — reducer: apply_snap [S6]

- [ ] Failing tests: ready → off with `user_stopped false` returns one `Note{"lobo stopped","stopped by itself (idle or expiry)"}`; after `begin_stop` → no note; booting snap with no boot_start → `boot_start_ms` = pod `started_at`; resumed boot (no up running, agent stage `download`, no steps) → steps rent…download marked, `at_s` = now − t0, container/tunnel included for cloud, skipped for local; off with no up → steps, download, boot_start cleared; status stage `verify` with download → `download` kept.
- [ ] Implement. `cargo test --manifest-path app/src-tauri/Cargo.toml store::apply_snap` → pass.
- [ ] Commit: `app: store apply_snap`.

## Task 14 — reducer: begin_up, handle_event, up_ended [S7–S9]

- [ ] Failing tests: `begin_up` local q6 → `UpRequest{provider:Some("local"), model:Some("q6"), ctx/source/cloud None}`; cloud vast q8 → `Some("vast")`, `Some("q8")` (port `upArgs` asserts LobocodeTests.swift:103-108); `last_detail` `starting llama.cpp…` / `renting vast…`; state cleared. `handle_event`: port `testUpEventsDriveSteps`; detail appended, tail kept at 6; err `"a\nb"` adds two lines, tail kept at 8; ready → `ready_url`, phase ready, `Note{"lobo ready","https://x/v1 · 0:00 · $0.70/h"}`; local ready → body ends `· local`. `up_ended(Some("boom"))` while booting with tail `["x",""]` → failed `x` + `Note{"lobo boot failed","x"}`; `up_ended(Some(..))` while ready or stopping → no change; `up_ended(None)` → no change.
- [ ] Implement. `cargo test --manifest-path app/src-tauri/Cargo.toml store::up` → pass.
- [ ] Commit: `app: store up events`.

## Task 15 — reducer: stop, dismiss, warning [S10, S11, S3]

- [ ] Failing tests: `begin_stop` returns `is_local` at press time and sets stopping + user_stopped; `stop_failed("down: x")` → failed; `stop_done` → off; `dismiss` → off; `set_warning` round trip; `poll_failed("x")` while loading → off + warning `x`, while ready → stays ready + warning.
- [ ] Implement. `cargo test --manifest-path app/src-tauri/Cargo.toml store::stop` → pass.
- [ ] Commit: `app: store stop/dismiss/poll failure`.

## Task 16 — Backend trait + CoreBackend + FakeBackend

**Files:** Create `app/src-tauri/src/backend.rs`.

- [ ] `CoreBackend` maps: `config` → `config::show` + `config::readiness`; `models` → `load_laptop` + `local::list(weights())`; `snapshot` → `load_laptop` → `deps_from_config(cfg, &self.wiring)` → `control::snapshot`; `up` → `resolve_up` → `control::up`; `down` → `control::down`; `api_key` → `config::values["LOBO_API_KEY"]`; `save` → `validate_set` (Err → `AppError{kind:"invalid"}`) → `config::save`. Config path: `LOBO_APP_CONFIG` if set, else `config::default_path()` [S23]. Wiring: `Wiring::new(abs(path), Spawner::app(current_exe()))`, built once in `CoreBackend::new(path)` and used for every call, so `LOBO_APP_CONFIG` reaches the spawned supervisor (`CLI.swift:15-20`, `provider.go:139-143`). Every `lobo_core::Error` → `AppError{kind: e.kind(), message: e.to_string()}`.
- [ ] `FakeBackend` (cfg(test)): scripted responses, records call names in order (`config`, `models`, `snapshot`, `up`, `down`), `up` returns a controllable UpOperation from the core testkit; tests control both events and completion.
- [ ] Failing test for `CoreBackend::save` with a temp config: invalid port → `AppError{kind:"invalid", message:"LOBO_LOCAL_PORT: whole number 1024-65534, or empty"}`, file untouched; valid set → file has the key, comments kept.
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml backend` → pass.
- [ ] Commit: `app: backend trait over lobo-core + fake`.

## Task 17 — Controller: loops + refresh [S2–S4]

**Files:** Create `app/src-tauri/src/controller.rs`, `app/src-tauri/src/notify.rs`.

- [ ] Failing tests (tokio paused clock, FakeBackend, recording Notifier, `emit` into a Vec): port `testModelsNotListedOnPlainRefresh` (LobocodeTests.swift:188-209): start → calls `config, models, snapshot`; plain `refresh(false)` → `snapshot`; `refresh(true)` with down → `snapshot, models`; `load_config(true)` → `config, models`. Not-ready config → phase setup, no `snapshot` call. Snapshot error → `warning` set, loading → off. Loop wakes after `poll_interval` (advance paused time 3 s in booting → second `snapshot`). `panel_shown(true)` → `refresh(true)` runs at once.
- [ ] Implement. Store mutex never held across `.await` (clippy `await_holding_lock` deny in this module).
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml controller::refresh` → pass.
- [ ] Commit: `app: controller poll loop and refresh`.

## Task 18 — Controller: start + stop [S7–S10]

- [ ] Failing tests: duplicate Start creates one operation. Controller owns UpOperation through cleanup. Stop cancels, drains events and waits for completion, then runs down (cloud second check after 10 s retained). At 120 s, show `stop: cleanup is still running` and keep stopping with Start disabled. Do not detach, abort, call down concurrently, or report OFF. Real-core tests with rent delays 20 s and 121 s prove the returned instance is deleted before down. Test a closed/full event stream, local preparation cancellation, panic, cleanup failure, repeated Stop, Dismiss and Quit during cleanup. Cleanup failure remains failed and cannot be overwritten by polling. Quit during pending up waits for cleanup before exiting; quit with an already-ready detached supervisor keeps existing Go behavior. After successful cleanup, refresh and enable Start.
- [ ] Start/Stop smoke must also run through the real built Tauri app and a fake local runtime; screenshots alone do not test commands.
- [ ] Implement. `cargo test --manifest-path app/src-tauri/Cargo.toml controller::up && cargo test --manifest-path app/src-tauri/Cargo.toml controller::stop` → pass.
- [ ] Commit: `app: controller start/stop sequence`.

## Task 19 — prefs [S15]

**Files:** Create `app/src-tauri/src/prefs.rs`.

- [ ] Failing tests: missing file → `target None`; save `Local` then load → `Local`; garbage file → `None` (no panic).
- [ ] Implement. Dir = Tauri `app_config_dir()` (= `~/Library/Application Support/io.github.1905.lobocode`). Render/test stores never read it (`Store::new(None, …)`).
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml prefs` → pass.
- [ ] Commit: `app: saved target pick`.

## Task 20 — Tauri commands

**Files:** Create `app/src-tauri/src/commands.rs`. Modify `lib.rs`.

- [ ] Every command from the Locked interfaces table, thin: call Controller / Backend / plugin, map errors to `AppError`. `copy_api_key` and `copy_text` use `tauri_plugin_clipboard_manager` from Rust. `reveal_config` → opener `reveal_item_in_dir`; `open_config` → opener `open_path` (no-op when the file does not exist); `choose_weights` → dialog `blocking_pick_folder` on a blocking task, start dir = arg `~`-expanded or listing weights. `free_bytes` → `local::free_bytes_nearest`. `gen_api_key` → `genkey::new_api_key`.
- [ ] Unit tests for the pure mapping helpers only (`AppError::from(lobo_core::Error)` keeps `kind`). Commands themselves are exercised in Task 40.
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml commands && cargo clippy --manifest-path app/src-tauri/Cargo.toml -- -D warnings` → pass.
- [ ] Commit: `app: tauri commands`.

## Task 21 — `--lobo-local-run` supervisor entry

**Files:** Create `app/src-tauri/src/supervisor.rs`. Modify `main.rs`.

- [ ] `main.rs`: if `args[1] == lobo_core::local::SUPERVISOR_ARG` and `args[2..4] == ["local", "run"]` → `std::process::exit(supervisor::main(&args[4..]))` before any Tauri code runs; `SUPERVISOR_ARG` without `local run` → stderr + exit 2 (no window, no tray, no Dock icon). `--render` is NOT handled here (renders come from the UI, Task 33).
- [ ] `supervisor::main`: `RunConfig::from_args(args, Manifest { version: app version, git_sha, .. })` error → print `lobocode --lobo-local-run: <err>` to stderr, return 2. Else tokio runtime, cancel on SIGINT/SIGTERM, `local::supervise`, Err → stderr + 1, Ok → 0.
- [ ] Failing test: `supervisor::main(&["--bogus".into()])` → 2. Integration-free.
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml supervisor` → pass.
- [ ] Commit: `app: --lobo-local-run re-exec runs the local supervisor`.

## Task 22 — Tray icon drawing [P21, D3]

**Files:** Create `app/src-tauri/src/icons.rs`, `app/src-tauri/examples/render_icons.rs`.

- [ ] Failing tests on the 32×32 RGBA output (2× scale of the Swift 16 pt geometry): ready → centre pixel `#2EE57A`; booting progress 0 → bottom inner row filled cyan `#00C8E6`, top inner row empty (0.12 floor); progress 1 → top inner row filled; failed → centre pixel red `#FF4D5E` (X crossing); setup → centre amber `#FFB020`, corner of inner rect empty; off → template flag `true`, frame pixels opaque black, centre empty; all others template `false`.
- [ ] Implement with tiny-skia (anti-aliased stroke 3 px at 2×, radius 6).
- [ ] Example: `cargo run --manifest-path app/src-tauri/Cargo.toml --example render_icons -- <dir>` writes `tray_{loading,setup,off,boot_0,boot_50,ready,stop,fail}.png` and `icon_1024.png` (Task 25).
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml icons` → pass.
- [ ] Commit: `app: tray icon drawing per phase`.

## Task 23 — Tray wiring

**Files:** Create `app/src-tauri/src/tray.rs`. Modify `lib.rs`.

- [ ] `TrayIconBuilder` with id `lobo`, initial icon = loading, `icon_as_template` per `tray_icon()`, title `lobo`, no menu, `show_menu_on_left_click(false)`.
- [ ] `update(app, &PanelState)`: set icon + template flag when phase or rounded progress (2% steps) changed; `set_title(Some(menu_text))` when changed. Called from the `emit` closure.
- [ ] Tray event: first call `tauri_plugin_positioner::on_tray_event(app, &event)`. Left click Up: `panel` visible → hide; else `move_window(Position::TrayCenter)`, show, focus, `controller.panel_shown(true)`.
- [ ] Unit test for the change-detection helper `fn tray_changes(prev: Option<&TrayLook>, next: &TrayLook) -> (bool /*icon*/, bool /*title*/)`.
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml tray` → pass.
- [ ] Commit: `app: tray item with status title and panel toggle`.

## Task 24 — Render fixtures from the reducer

**Files:** Create `app/src-tauri/tests/fixtures.rs`, `app/ui/src/fixtures/panel_*.json`, `settings.json`.

Each fixture file: `{ "name": …, "now_ms": <fixed>, "state": PanelState }`. `now_ms` = `2026-09-29T12:00:00Z`. Scenarios = `Renderer.swift:40-124` one-to-one (same values, same names), built by driving `Store` methods (not by writing `PanelState` by hand), plus the states the Swift renders missed:

| Name | Source |
|---|---|
| `off`, `boot`, `ready`, `fail`, `setup`, `off_local`, `boot_local`, `verify_local`, `ready_local`, `fail_local` | Renderer.swift:50-123 |
| `loading` [P1], `stopping` [P14 cloud], `stopping_local`, `off_nokeys` [P4], `off_one_provider` [P5 row], `off_local_nomodels` [P7], `boot_verify_sha` [P9 verifying], `fail_pod` [P15 STOP provider], `ready_warning` [P16], `ready_kill_soon` (cloud, 240 s left) | new |

`settings.json` = `{ config: sampleConfig, readiness, models: sampleModels, local_supported: true }` from Renderer.swift:29-38.

- [ ] Test `write_render_fixtures` writes the files; test `render_fixtures_cover_inventory` asserts the name list above and that every `Phase` variant appears at least once.
- [ ] Makefile `app-fixtures: cargo test --manifest-path app/src-tauri/Cargo.toml --test fixtures`. Verify: run twice → `git status --porcelain app/ui/src/fixtures` shows no change on the second run.
- [ ] Commit: `app: render fixtures generated from the reducer`.

## Task 25 — App icon [P34]

**Files:** Modify `icons.rs`. Create `app/src-tauri/icons/*` (generated).

- [ ] Failing tests on `app_icon_1024()`: pixel (512, 100) = bg `#0B0D10`; (10, 10) transparent (outside the 824 tile); centre of the first `L` cell = cyan; centre of the first filled `C` cell = magenta; a `.` cell centre = bg. Glyph rows = PixelGlyph.swift:30-31 exactly.
- [ ] Implement (tile r185 approximated as a rounded rect; "continuous" squircle not reproduced — note it in the review page).
- [ ] `make app-icons` = `cargo run --manifest-path app/src-tauri/Cargo.toml --example render_icons -- bin/app-renders && cargo tauri icon bin/app-renders/icon_1024.png -o app/src-tauri/icons`. Replaces `macos/make_icns.py` (sips + iconutil).
- [ ] Verify `ls app/src-tauri/icons/icon.icns` exists. `cargo test --manifest-path app/src-tauri/Cargo.toml icons::app` → pass.
- [ ] Commit: `app: LC pixel monogram icon via tiny-skia + cargo tauri icon`.

## Task 26 — TS: theme tokens + fmt [T1–T16, F1–F3]

**Files:** Create `app/ui/src/lib/theme.css`, `fmt.ts`, `fmt.test.ts`.

- [ ] `theme.css`: `:root` custom properties `--bg --card --line --text --dim --faint --green --cyan --magenta --amber --red`, `--copper: linear-gradient(90deg, var(--cyan), #8A7BFF, var(--magenta))`, `--mono: ui-monospace, "SF Mono", Menlo, monospace`, `--fs-9 … --fs-22`. `color-scheme: dark`. `@media (prefers-reduced-motion: reduce)` stops every animation. `.render` class (set by the render route) also stops them and shows the settled frame.
- [ ] `fmt.ts`: `duration(s)`, `gb(bytes)`, `bar(frac, width)`, `tps(v?: number)` (≥100 `%.0f`, else `%.1f`, missing `—`), `usd(v)` `$%.2f`.
- [ ] Failing vitest: `duration` over `time_cases.json`; `gb(22082528352)` `22.1`; `bar(0.5,4)` `▓▓░░`; `bar(NaN,2)` `░░`; `tps(503.9)` `504`, `tps(45.1)` `45.1`, `tps()` `—`.
- [ ] `pnpm -C app/ui test fmt` → pass.
- [ ] Commit: `ui: theme tokens and formatters`.

## Task 27 — TS: view mapping [P5–P17]

**Files:** Create `app/ui/src/lib/view.ts`, `view.test.ts`.

Locked exports (pure; `now` passed in):
```ts
headerDetail(s: PanelState): string
limits(values: Record<string,string>): string
modelRows(s: PanelState): { id: string; picked: boolean; size: string; state: 'on'|'partial'|'missing'; pct?: number }[]
localFooter(l: Listing): string
stepRows(s: PanelState): { step: Step; label: string; mark: 'ok'|'cur'|'wait'; at?: string }[]
downloadLine(s: PanelState): null | { kind: 'verify' } | { kind: 'sha'; bar: string } | { kind: 'bytes'; bar: string; gb: string; mbps: string; mbpsTone: 'text'|'green'|'amber'; eta?: string }
bootElapsed(s: PanelState, nowMs: number): string                     // "T+1:12"
ready(s: PanelState, nowMs: number): { endpoint: string; apiKey: string; gen: string; prompt: string;
  mem?: { label: 'memory'|'vram'; bar: string; text: string; gpu?: { text: string; hot: boolean } };
  kill?: { label: string; text: string; warn: boolean }; uptime: string; cost: string }
fail(s: PanelState): { message: string; tail: string[]; primary: { label: string; action: 'stop'|'start'; tone: 'red'|'green' } }
stopping(s: PanelState): string
```
- [ ] Failing vitest, fixtures from `app/ui/src/fixtures/panel_*.json` and P1's `crates/lobo-proto/fixtures/{snap_running,snap_down,listing,listing_empty,config_show}.json`: `headerDetail` boot = `vast · offer 51401937, 18877 Mbps down, California, US · $0.73/h`; off cloud `no pod · $0.00/h`; off local `""`. `limits({})` = `≥100MB/s idle 30m max 12h`; `LOBO_IDLE_MIN "0"` → `30m`. `modelRows(off_local)`: q6 picked on disk, q8 partial 43. `stepRows(boot)`: rent…gpu ok with times, download cur, load/ready wait. `downloadLine(boot)` bytes `12.4/28.6G`, `713MB/s` green, eta present; `boot_local` `88MB/s` tone text; `verify_local` → `verify`; `boot_verify_sha` → `sha`. `ready(ready)`: gen `45.1`, prompt `504`, vram `29316/32607` → `28.6/31.8 GB`, gpu `gpu 87%` hot, kill label `idle-kill ` over `time_cases.kill_left`, cost `$1.60` at 8342 s × $0.69/h; `ready_local`: `memory`, no gpu, `idle-stop `, cost `local · $0`; `ready_kill_soon` warn. `fail(fail_pod)` primary `STOP runpod` red; `fail(fail)` `RETRY` green; tail = last 5.
- [ ] Implement. `pnpm -C app/ui test view` → pass.
- [ ] Commit: `ui: PanelState → view mapping`.

## Task 28 — TS: settings form logic [P27–P32]

**Files:** Create `app/ui/src/lib/settings.ts`, `settings.test.ts`.

Locked exports:
```ts
export const PLAIN_KEYS: string[]      // SettingsView.swift:228-230, same order
export const SECRET_KEYS: string[]     // RUNPOD_API_KEY, VASTAI_API_KEY, CF_TUNNEL_TOKEN
export type Fields = { secrets: Record<string,string>; plain: Record<string,string>; newApiKey?: string }
loadFields(c: ConfigShow | undefined): Fields
changes(f: Fields, current: Record<string,string>): Record<string,string>
secretHint(c: ConfigShow | undefined, key: string): string   // "not set" | "<masked>  (empty = keep, - = remove)"
providerTargets(r: Readiness): { options: string[]; def: string } | null   // null = hidden (≤1 option)
pickerValue(f: Fields, key: string, def: string): string
savedMessage(n: number): string        // "saved 1 key" / "saved 3 keys"
maskNew(key: string): string           // "sk-0…cdef  (new, unsaved)" (F4 rule)
```
- [ ] Failing vitest: port `testSettingsChanges` + weights trim (LobocodeTests.swift:211-236) → exact result maps; `providerTargets` local+runpod+vast → `["local","runpod","vast"]` def `runpod`; local only → `null`; `savedMessage(1)` `saved 1 key`.
- [ ] Implement. Validation is NOT here: `config_save` returns the Rust message (Task 4). `pnpm -C app/ui test settings` → pass.
- [ ] Commit: `ui: settings form diff`.

## Task 29 — TS: api + boot

**Files:** Create `app/ui/src/lib/api.ts`. Modify `main.ts`.

- [ ] `api.ts`: one typed wrapper per command (names = commands table), `onState(cb)` over `listen('lobo://state')`, `inTauri()` = `'__TAURI_INTERNALS__' in window`. Outside Tauri every call throws `not in tauri` (render mode never calls them).
- [ ] `main.ts`: `?view=panel` → Panel, `?view=settings` → Settings, `?view=render` → Render. Panel view: `get_state()` then `onState`; a 1 s `now` ticker (for T+, countdowns); `ResizeObserver` on the root → `getCurrentWindow().setSize(new LogicalSize(340, h))`.
- [ ] `pnpm -C app/ui check && pnpm -C app/ui build` → 0 errors.
- [ ] Commit: `ui: api wrappers and view entry`.

## Task 30 — TS: widgets [T7–T14]

**Files:** Create `app/ui/src/widgets/*.svelte`.

- [ ] `Logo` (art = Theme.swift:41-45 byte-exact, gradient via `background-clip: text`), `RasterBar {active}`, `Scanlines`, `GlitchText {text, color}` (T10 timings), `Cursor`, `BracketButton {label, tone, wide, disabled, onclick}`, `LinkButton {label, tone, onclick, disabled}`, `BracketPicker {label, options, value, onpick}`.
- [ ] Svelte 5 runes only (`$props`, `$state`, `$derived`). No component tests; covered by renders.
- [ ] `pnpm -C app/ui check` → 0 errors, 0 warnings.
- [ ] Commit: `ui: widgets (logo, raster bar, glitch, buttons, picker)`.

## Task 31 — TS: panel header, footer, setup, start [P1–P7, P16–P19]

**Files:** Create `app/ui/src/panel/{Panel,Header,Footer,SetupCard,StartCard,LocalStart}.svelte`.

- [ ] Strings exactly as the inventory. `Panel` switches on `phase.kind`. Enter triggers START in off states (`keydown` on window, only when the START button is shown). ⌘, → `open_settings`, ⌘q → `quit`.
- [ ] Model picker options = `state.catalog_ids` (DEFAULT_MODEL first → `q8 q6`, matches P5/P7).
- [ ] `pnpm -C app/ui check` → clean.
- [ ] Commit: `ui: panel header/footer/setup/start`.

## Task 32 — TS: boot log, ready, fail, stopping [P8–P15]

**Files:** Create `app/ui/src/panel/{BootLog,ReadyCard,FailCard}.svelte`; stopping + loading inline in `Panel`.

- [ ] `copy` → `copied` (green) for 1.2 s per row; endpoint → `copy_text`, key → `copy_api_key`.
- [ ] `pnpm -C app/ui check` → clean.
- [ ] Commit: `ui: boot log, ready card, fail card`.

## Task 33 — TS: settings view + render route [P24–P33, S24]

**Files:** Create `app/ui/src/settings/Settings.svelte`, `app/ui/src/render/Render.svelte`.

- [ ] Settings: on mount `config_show` + `local_models`; weights free: saved folder → listing number; typed → `free_bytes` debounced 300 ms; `generate` → `gen_api_key`; save flow = P32; ⌘S saves. Error from `config_save` → `save failed: <message>` red, except `kind == "invalid"` → message alone red (P33 texts). Width 520, scrolls in the window (640 high).
- [ ] Render route: `?view=render&state=<name>` shows one panel fixture at 340 px wide on the bg, clock frozen at `now_ms`, `.render` class on. `?view=render&state=settings` shows Settings from `settings.json` without calling Tauri. `?view=render&state=menubar_<name>` shows the menu bar strip: bg `#2A2A2E`, padding 3/8, the tray PNG for that phase from `app/ui/public/tray/` (copied there by `make app-render`, Task 35), then `menu_text` in 12 px mono white (Renderer.swift:127-137). `?view=render` alone lists every link.
- [ ] `pnpm -C app/ui check && pnpm -C app/ui build` → clean.
- [ ] Commit: `ui: settings window and render route`.

## Task 34 — Windows, launch, reopen [P23, P24, S4, S24, D2]

**Files:** Create `app/src-tauri/src/windows.rs`. Modify `lib.rs`.

- [ ] `setup`: `set_activation_policy(Accessory)`; build tray; create Controller (emit → `app.emit("lobo://state", s)` + `tray::update`); request notification permission; `spawn_loops`.
- [ ] `panel` window: url `index.html?view=panel`, 340×(content), `decorations(false)`, `resizable(false)`, `always_on_top(true)`, `skip_taskbar(true)`, `visible(false)`, bg `#0B0D10`. `WindowEvent::Focused(false)` → hide + `panel_shown(false)`.
- [ ] `main` window (window mode): url `index.html?view=panel`, title `lobocode`, `title_bar_style(Overlay)`, `hidden_title(false)`, `theme(Dark)`, bg `#0B0D10`, not resizable, centred on first show, reused (`CloseRequested` → `prevent_close` + hide + `panel_shown(false)`), top padding for the 28 px titlebar via `?view=panel&chrome=window`.
- [ ] `settings` window: url `index.html?view=settings`, title `lobocode · config`, 520×640, same style, reused.
- [ ] Launch: unless `--background` in argv → show `main`. `--settings` → show `settings` after 500 ms. `RunEvent::Reopen { .. }` (macOS) → show `main`. ⚠ `RunEvent::Reopen` field names: verify against docs.rs for the pinned tauri version before coding; tauri 2 has `Reopen { has_visible_windows, .. }` (from memory, not verified in this plan).
- [ ] `open_settings` command shows `settings`.
- [ ] Unit test: argv flag parser `fn launch_flags(args: &[String]) -> LaunchFlags { background, settings }`.
- [ ] `cargo test --manifest-path app/src-tauri/Cargo.toml windows && cargo clippy --manifest-path app/src-tauri/Cargo.toml -- -D warnings` → pass.
- [ ] Commit: `app: panel popover, window on launch/reopen, settings window`.

## Task 35 — Makefile

**Files:** Modify `Makefile`.

Targets (all `mv`-to-trash, never `rm`, like today):
- `app-test: cargo test --manifest-path app/src-tauri/Cargo.toml && pnpm -C app/ui test`
- `app-lint: cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings && pnpm -C app/ui check`
- `app-icons:` as Task 25.
- `app-fixtures:` as Task 24.
- `app-render: app-icons app-fixtures` then copy `bin/app-renders/tray_*.png` → `app/ui/public/tray/`, then prints `run: pnpm -C app/ui dev, open http://localhost:5173/?view=render`.
- `mac:` `pnpm -C app/ui install --frozen-lockfile && pnpm -C app/ui build && (cd app && cargo tauri build --bundles app --config '{"version":"$(APP_VERSION)"}')`, then move `app/src-tauri/target/release/bundle/macos/lobocode.app` to `$(MAC_APP)` (old one to trash first). `APP_VERSION` = `$(VERSION)` without `v` when it is semver-like, else `0.0.0-dev` (tauri needs semver).
- `dmg: mac` — unchanged body (D4).
- `install-mac: mac` — unchanged body.
- `mac` no longer depends on `build-lobo` and no longer copies `lobo` into Resources (spec goal: no CLI in the app).
- [ ] Verify: `make -n mac dmg install-mac` prints the expected commands. The real `make mac` runs in Task 40 (orchestrator).
- [ ] Commit: `make: mac/dmg/install-mac build the Tauri app`.

## Task 36 — CI: rust.yml app job + release dmg job

**Files:** Modify `.github/workflows/rust.yml`, `.github/workflows/release.yml`.

- [ ] `rust.yml` path filters include `app/**`, `crates/**`, `Cargo.*`, `.cargo/**`, `rust-toolchain.toml`, `Makefile` and the workflow. Ubuntu jobs still build only the root workspace. New job `app` on `macos-15`: rust toolchain, `Swatinem/rust-cache@v2` with `workspaces: "app/src-tauri -> target"`, `pnpm/action-setup@v4`, `actions/setup-node@v4` (node LTS, `cache: pnpm`, `cache-dependency-path: app/ui/pnpm-lock.yaml`), `pnpm -C app/ui install --frozen-lockfile`, `make app-lint app-test`, `make app-fixtures && git diff --exit-code app/ui/src/fixtures app/ui/src/gen`.
- [ ] `release.yml` `dmg` job: drop `setup-go`; add rust toolchain + `Swatinem/rust-cache@v2` + pnpm + node + `cargo install tauri-cli --version "<pinned>" --locked`; `make dmg`; upload step unchanged.
- [ ] `actionlint` on both files if installed, else say it was not run. Clean-checkout check (orchestrator): push, `gh run watch` → job `app` green from an empty cache.
- [ ] Commit: `ci: app job on macos-15; release dmg from the Tauri build`.

## Task 37 — Renders + review page (orchestrator)

- [ ] `make app-render`; start `pnpm -C app/ui dev` in the background.
- [ ] Screenshots via the `/playwright-cli` skill (never Playwright MCP): one PNG per render URL at device scale 2, panel 340 px wide, settings 520 px, menu bar strips. Use the WebKit engine if playwright-cli offers it (Tauri renders in WKWebView); else Chromium and say so on the review page. Output `bin/app-renders/ui/<name>.png`.
- [ ] Look at every screenshot yourself. Fix overflow, clipping, wrong font, missing state, then re-shoot.
- [ ] `tools/render_review.py` (Python, per the scripting rule): pairs `p5-baseline/panel_<name>.png` (Swift) with `bin/app-renders/ui/<name>.png` (Tauri) per state, new-only states on their own, tray PNGs, icon; notes the WebKit/Chromium fact and the icon squircle note; writes one HTML page. Publish as a private artifact.
- [ ] `/notify`: "P5 renders ready: Swift vs Tauri per state, ready for QA; continuing full auto" with the artifact link.
- [ ] Full-auto authorization covers delivery. Send the renders for QA, continue with verified parity, and apply any feedback when it arrives. No approval wait.

## Task 38 — Remove `macos/` (after render verification)

- [ ] `mkdir -p /tmp/trash && mv macos /tmp/trash/macos.$(date +%Y%m%d-%H%M%S) && git add -A`.
- [ ] `grep -rn "macos/" Makefile .github README.md` → no hits (except history in `plans/`).
- [ ] Commit: `app: remove the Swift app (replaced by app/)`.

## Task 39 — README + docs images

**Files:** Modify `README.md`, `docs/img/{menubar_ready,panel_boot,panel_ready,settings}.png`.

- [ ] App section: drop "It runs the same `lobo` CLI"; say it uses the same config file and runs the local model itself. Install text (dmg, Gatekeeper, removable volume) unchanged. Development line: `app/` Tauri + Svelte, `make app-test`. Cold-audience register (no contractions).
- [ ] Copy the approved renders to `docs/img/` (same names).
- [ ] Commit: `readme: Tauri app`.

## Task 40 — Orchestrator app runs (no implementer; free, no cloud)

- [ ] `make install-mac` → `/Applications/lobocode.app` (or `~/Applications`).
- [ ] Launch from Finder: `open /Applications/lobocode.app` → `main` window shows the panel [P23]. Close it, `open` again → it reopens (Reopen) [P23]. Tray item visible with title; click → popover under the item [D2].
- [ ] Tray title width jump check across `off → boot → 42% → 27m` (D3). Screenshot the menu bar. If the user rejects it, add the D3-B task.
- [ ] Setup state with `LOBO_APP_CONFIG=/tmp/lobo-p5-empty.env open …` (missing file) → SETUP card; Settings opens; SAVE creates the file with a generated API key; state leaves setup.
- [ ] Local boot on the real config (free): pick local q6 → START → steps `start metal model load ready`, ready card, endpoint `http://127.0.0.1:8931/v1`. `ps -ww -o command= -p <pid from state file>` shows `…/lobocode --lobo-local-run … --boot-id <id>`. `curl -s -H "Authorization: Bearer $key" http://127.0.0.1:8931/v1/models` → 200 (key read from the config file, never from the OS env). Copy buttons put the endpoint and the clear key on the clipboard.
- [ ] STOP → phase off within ~5 s, supervisor process gone, state file removed. `lobo status` from the P4 CLI agrees (off).
- [ ] Quit the app while local runs → supervisor keeps running (setsid, like the CLI); relaunch → app shows ready from the snapshot.
- [ ] Notifications: ready, stopped-by-itself (set `LOBO_IDLE_MIN=1` locally, wait), boot failed (`LOBO_LOCAL_PORT` pointing at a port in use) [S6, S8, S9].
- [ ] No cloud boot in P5. Cloud paths are covered by the reducer tests with fakes; the live cloud check is P6.
- [ ] `make dmg` → `bin/lobocode.dmg` opens, shows the app + Applications link.

## Task 41 — Phase close (orchestrator)

- [ ] Every inventory row P1–P34, S1–S24, T1–T16, F1–F5 ticked, each with the task or render that proves it. Any row not proven → new task, not a tick.
- [ ] Swift test → Rust/TS home check: every `LobocodeTests.swift` test (17) has a named port (table below). Count matches Task 0.
- [ ] `make rust-lint rust-test app-lint app-test && git status --porcelain` → green, clean.
- [ ] Reconcile spec: P5 "As-built notes" (Rust controller, two windows, D3 title font, contract v1.1 changes).
- [ ] Plan status → `done`. `/notify`: "Rust P5 done on feat/rust: Tauri app, renders checked, local boot OK. Continuing P6 under full-auto authorization."

---

## Swift test → new home

| Swift test (LobocodeTests.swift) | New home |
|---|---|
| testDecodeReadyStatus, testDecodeOffStatus, testDecodeUpEvents, testNanosecondDates | lobo-proto round-trip fixtures (P1) |
| testDerive | store::derive (Task 10) |
| testUpEventsDriveSteps | store::up + store::menu (Tasks 12, 14) |
| testDecodeModels | lobo-proto `listing.json` (P1) + fmt.test `gb` (Task 26) + store model state (Task 11) |
| testLocalSteps | types (Task 9) |
| testLocalBoot | store::up + store::menu (Tasks 12, 14) |
| testLocalEndpointWithoutReadyEvent | store::endpoint (Task 12) |
| testDefaultTarget | store::target (Task 11) |
| testTargetPersists | prefs (Task 19) |
| testModelsNotListedOnPlainRefresh | controller::refresh (Task 17) |
| testSettingsChanges | settings.test (Task 28) + lobo-core validate_set (Task 4) |
| testSettingsLocal | lobo-core validate_set + free_bytes_nearest (Tasks 4, 7) + settings.test (Task 28) |
| testReadyNeedsAPIKey | lobo-core readiness (Task 3) |
| testFormat | fmt.rs / fmt.test (Tasks 12, 26) + new_api_key (Task 4) |

---

## Historical self-review (before the current correction)

- Spec P5 → tasks: menu bar item + status word (Tasks 22, 23); panel states off/boot/ready/fail/setup cloud + local (Tasks 10–15, 27, 31, 32); Settings, all keys, weights picker (Tasks 28, 33, 20 `choose_weights`); notifications (Tasks 14, 17, 18); renders of every state for review (Tasks 24, 33, 37); calls lobo-core directly (Task 16); supervisor re-exec (Tasks 6, 7, 21); Finder/reopen shows the panel (Task 34); positioner TrayCenter (Task 23); ts-rs types (Tasks 8, 9); unsigned dev build + dmg (Tasks 35, 36); vitest mapping (Tasks 26–28); no Windows/Linux (File map "Out of scope").
- Names match contracts.md v1.0: `config_show`, `config_save`, `local_models`, `catalog`, `AppError { kind, message }`, `UpRequest` in lobo-proto, `SUPERVISOR_ARG`, `Spawner { exe, args_prefix }`, `control::up/snapshot/down`, `config::show/save/values/parse_local_port/default_path`, `local::list/supervise/RunConfig`, `Error::kind`. Departures are listed under "Contract additions", not silently used.
- No TODO or placeholder, except "Pinned versions" (filled by Task 2 from lockfiles, same as P1) and one flagged unverified API detail (`RunEvent::Reopen` field names, checked in Task 34 before coding).
- Task sizes: Tasks 1, 24, 27, 34, 37 and 40 are larger than 5 minutes. They are listed as single tasks because splitting them would separate a test from what it tests.

## Contract additions (for contracts.md v1.1)

1. `lobo_proto::{UpRequest, Readiness}` — shapes in "Locked interfaces". `UpRequest` fields are all `Option` (None = config default); contract v1.0 did not say.
2. `lobo_core::config::{readiness, validate_set}`. (`new_api_key` = P3 `genkey::new_api_key`; `mask` = P3 `config::mask`.)
3. `lobo_core::control::resolve_up(cfg, cfg_path, req) -> Result<UpOpts>` — `applyDefaults` + `checkTarget` live in `cmd/lobo` today; the app needs them without the CLI. App only; contract v1.1 adds a `supported: fn() -> Result<()>` param.
4. (dropped in v1.1) `deps_with_spawner` — P3's `deps_from_config(cfg, &Wiring)` already carries the spawner and the config path.
5. (dropped in contracts v1.1, resolved conflict 3) `is_supervisor` accepting `SUPERVISOR_ARG`: moot, `Spawner::app` already puts `local run` in the argv.
6. `lobo_core::local::free_bytes_nearest`. (Parsing = P3 `RunConfig::from_args`.)
7. App command surface: add `get_state, start, stop, dismiss, choose_target, set_provider, set_model, refresh, copy_api_key, copy_text, free_bytes, gen_api_key, choose_weights, open_settings, reveal_config, open_config, quit` and event `lobo://state` (`PanelState`). Drop from the TS surface: `snapshot`, `up` + `lobo://up`, `cancel_up`, `down` (the Rust controller calls them; D1).
8. `AppError` lives in the app crate (`app/src-tauri/src/types.rs`), exported to `app/ui/src/gen/`.

## Historical spec issues (resolved by execution.md unless marked pending)

1. Contract v1.0 streams `up` to TS. That puts the boot state machine in a webview that is hidden most of the time, while the tray and notifications need it. Plan moves it to Rust (D1). Contract change #7.
2. Supervisor identity is argv `local run` (`internal/local/provider.go:268-280`). The spec's `--lobo-local-run` re-exec breaks it unless P3 changes the check (#5). If P3 already shipped, Task 6 changes it here.
3. The spec says "no CLI subprocess", but `applyDefaults`/`checkTarget` and the local run flag parsing are CLI code today. They move to lobo-core (#3, #6). P4 plan must be told.
4. `rust.yml` (P1) runs `cargo test --workspace` on ubuntu. Adding the Tauri crate breaks it (webkit2gtk). Task 36 excludes it and adds a macOS job.
5. Readiness: Swift `cloudReady` checked only that 4 keys are set. The CLI (`RequireCloud`) also rejects a non-URL `LOBO_BUCKET_URL`. The plan uses the CLI rule, so a bad bucket URL now shows the SETUP path instead of a failed boot. Small behaviour change; accepted as shared CLI validation under full-auto execution.
6. Target pick was in macOS `UserDefaults` (`lobo.target`). The Tauri app stores it in `prefs.json`. The old pick is not migrated; first launch uses the default-target rule. Say so in the release notes, or add a one-line `defaults read` import (not planned).
7. Tray title font: tauri's `set_title` uses the system font, not monospaced digits (D3). Not measured. Decided at Task 40 with the user.
8. `cargo tauri` on this Mac is 1.5.11 (measured). P1 Task 0 only checks presence; P5 Task 0 installs 2.x.
9. Spec "weights picker" read as the Settings weights-folder row (text + `[choose…]` + free space). The model picker is separate (panel, Settings). If the spec meant a model download picker, that is a new feature and out of scope.
10. Render review adds 10 states the Swift renders never covered (Task 24 list). No new UI, only more frames to review.
11. Screenshots come from a browser, the app renders in WKWebView. WebKit engine if playwright-cli supports it; else Chromium, stated on the review page.

## Review fixes v1.1

Codex plan review of the v1.0 bundle (session f79006e4, 6/10). Confirmed findings applied here:
- F1 (high): v1.1 drain-only approach was incomplete. v1.2 adds owned completion and a 121 s regression; timeout never releases operation ownership.
- F2 (high): no `deps_with_spawner`; the app uses P3 `deps_from_config(cfg, &Wiring)` so `LOBO_APP_CONFIG` reaches the supervisor; spawner prefix = P3 `Spawner::app` (`--lobo-local-run local run`).
- F3 (high) + open item 3: `app/src-tauri` is its own cargo workspace. Pod Dockerfile and ubuntu CI stay untouched.
- F4 (high): app CI job installs `app/ui` deps (`--frozen-lockfile`, cache path set) before lint/test.
- F9 (med): local endpoint comes from the running instance; config port only when nothing runs.
- Open item 1: `macos/` removal stays here (Task 38); P6 only checks it is gone.

Contract alignment (contracts v1.1):
- Item 12: locked `config` block: key generator = `genkey::new_api_key`; `config::mask` line dropped (P3 ships it).
- Item 13: locked `control` block: `resolve_up(cfg, cfg_path, req, supported)`, app only.
- Item 14: locked `local` block: `RunConfig::from_args(args, version)`; `is_supervisor(pid, boot_id, ps)`, `local run` + `--boot-id` only.
- Item 15: `supervisor.rs`: args after `SUPERVISOR_ARG local run` (`args[4..]`), `RunConfig::from_args(.., Manifest)`.
- Item 16: File map + Tasks 3, 4, 5 use the P3 layout (`src/config/mod.rs`, `src/control/precheck.rs`).
- Item 17: Task 4 implements `validate_set` only; `new_api_key`/`mask` rows test the P3 functions.
- Item 18: Task 5: `resolve_up` composes `apply_defaults` + `check_target`; nothing moves out of lobo-cli.
- Item 19: Task 6 tests: `--lobo-local-run local run --boot-id B1` → true; `--lobo-local-run --boot-id B1` → false; called with `&command_of`.
- Item 20: Task 7: `RunConfig::from_args([...], Manifest::default())`; missing `--boot-id` → Ok.
- Item 21: Task 20: `gen_api_key` → `genkey::new_api_key`.
- Item 22: Task 21: check `args[2..4] == ["local","run"]`, pass `&args[4..]` to `RunConfig::from_args` with the app `Manifest`.
- Items 23–26: Contract additions 2, 3, 5, 6 rewritten to match contracts v1.1.
- Doc ref: header `contracts.md v1.0` → `v1.1`.

## Current execution additions (v1.2)

- App Cargo.toml declares its own `[workspace]`, package edition/rust-version and dependencies. Root excludes app/src-tauri. Commit both Cargo.lock files. Never inherit workspace.dependencies from a different workspace. Root crates retain their root workspace metadata. Verify both cargo metadata commands from a clean checkout.
- Tauri commands run from `app/`. Bundle output is `app/src-tauri/target/release/bundle/macos/lobocode.app`. Do not set a shared target directory. Pin pnpm and Node versions; both CI jobs install with the frozen UI lockfile.
- Task 36 also runs `make mac`, validates the bundle binary, runs TS export for proto and app types, and checks generated files. CI must trigger on UI-only changes. Rebuild the pod image after introducing the app workspace.
- App TS export uses an explicit absolute export directory for each crate, so root `.cargo/config.toml` does not send app types into `src/proto`. Check generated types from a clean tree without pre-existing output.

## Baseline preparation — 2026-09-30

While P4 release packaging runs, saved the unchanged Swift app’s 22 reference renders in `p5-baseline/`. Release build and all 17 Swift tests pass. No live config or provider was used. P4 completion and the Tauri scaffold remain pending.
