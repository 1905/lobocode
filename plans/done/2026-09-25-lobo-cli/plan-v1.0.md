# lobo installable binary — Implementation Plan v1.0

**Date:** 2026-09-25
**Status:** done
**Spec:** ./spec.md

**Goal:** `make install` → `lobo` anywhere; `lobo config` TUI for keys + defaults; `up` uses them; goreleaser/brew prepared, not released.
**Architecture:** config moves to `~/.config/lobo/config.env` (dotenv, same keys). New `internal/configtui` (huh v2 on bubbletea v2) edits it through `config.Save`, which keeps unknown lines. `up` resolves flag → config → built-in.
**Tech:** Go 1.26, cobra, charm.land/{bubbletea,lipgloss,huh}/v2, goreleaser v2.

> Executed by the orchestrator (Opus 5.5) directly on branch `feat/cli` off `master` after feat/vast merges. One Astra review at the end (user rule for this project: no per-task reviews).

## Preconditions
- feat/vast merged into master; tree clean; `git checkout -b feat/cli master`.
- Baseline: `go build ./... && go test ./...` green.

## Locked interfaces

```go
// internal/config
func DefaultPath() string                              // $XDG_CONFIG_HOME/lobo/config.env | ~/.config/lobo/config.env
func LoadLaptop(path string) (Laptop, error)           // unchanged signature; RunPod key optional, ≥1 provider key
func Save(path string, set map[string]string) error    // in-place update, keep unknown lines/comments, header if new, atomic, 0600
func Values(path string) (map[string]string, error)    // raw dotenv map ({} if missing)
type Defaults struct { Provider, Model, Cloud string; Ctx, IdleMin, MaxHours, MinMBps int; VastMaxDPH float64 }
func (l Laptop) Defaults() (Defaults, error)           // validated; zero = built-in
func (l Laptop) Providers() []string                   // names with a key, stable order runpod, vast
func (l Laptop) DefaultProvider() string               // LOBO_PROVIDER if keyed, else runpod if keyed, else vast

// internal/configtui
func Run(path string, cur map[string]string) (map[string]string, bool, error) // changed values, saved?
func Mask(s string) string                                                    // "abcd…wxyz"; short → "••••"

// cmd/lobo
func applyDefaults(cmd *cobra.Command, o *control.UpOpts, d config.Defaults, cfg config.Laptop) error
```

## Tasks

- [ ] **C1 config path + optional RunPod** — `internal/config/path.go`, `laptop.go` (+tests): `DefaultPath`, `Values`, `RUNPOD_API_KEY` not required, error "config: set RUNPOD_API_KEY or VASTAI_API_KEY"; `Providers()`, `DefaultProvider()`, `Defaults()` with validation (model q8|q6, cloud secure|community, provider runpod|vast, ints ≥0, dph >0). `cmd/lobo/main.go`: `--config` default `DefaultPath()`, hidden `--env` alias; missing file → "no config at <path> — run `lobo config`"; perm warning if mode & 077. `providers(cfg)` only adds runpod when keyed.
- [ ] **C2 Save** — `internal/config/write.go` (+tests): round-trip, unknown lines kept, header + grouped layout for new file, `-`/empty semantics are the TUI's job (Save sets exactly what it gets; empty value = delete key line).
- [ ] **C3 up defaults** — `applyDefaults` + table test (`cmd/lobo/defaults_test.go`): each flag `Changed` vs config vs built-in; provider pick cases; unkeyed provider error.
- [ ] **C4 config TUI** — `internal/configtui/{form.go,theme.go,mask.go}` (+tests for pure parts): groups Providers / Access / Defaults / Confirm; header with path + "edit by hand" note; password inputs with keep/clear semantics; LOBO API key keep/generate (move generator into `internal/config` or reuse `genkey.go` func). `cmd/lobo/config.go`: `config` (TTY → form → Save; non-TTY → show), `config path`, `config show`. Manual check in tmux: `tmux new -d -s lobocfg 'lobo config --config /tmp/lobo_cfg_test.env'`, capture pane.
- [ ] **C5 help + version** — `cmd/lobo/help.go`: styled usage template, groups, examples, no-config hint; plain when non-TTY/`NO_COLOR`; golden test. `release` hidden. `version` prints `version commit date`.
- [ ] **C6 Makefile + install** — targets per spec; `install` with PATH hint; `make install && cd /tmp && lobo` works. Copy repo `.env` → `~/.config/lobo/config.env` (0600) if the target does not exist.
- [ ] **C7 goreleaser + workflow** — `.goreleaser.yaml`, `.github/workflows/release.yml`; `goreleaser check`; `goreleaser release --snapshot --clean --skip=publish` → 4 archives + `dist/homebrew/lobo.rb`; `dist/` in `.gitignore`. No tag.
- [ ] **C8 remotes** — `~/dev/homebrew-tap`, `~/dev/rival`: `git remote set-url origin https://github.com/1905/<repo>.git`; `git fetch origin` succeeds.
- [ ] **C9 docs** — README install/config/brew + release blockers; `.env.example` defaults block.
- [ ] **C10 exit** — `go build ./... && go test ./... && golangci-lint run ./...`; one `/rival-astra review` on feat/cli → verify + fix → merge → push → notify.

## Self-review
- Spec rows → C1 (path.go, laptop.go, main.go config), C2 (write.go), C3 (up defaults), C4 (configtui, config.go), C5 (help, version), C6 (Makefile), C7 (goreleaser, workflow), C8 (remotes), C9 (README/.env.example). e2e keeps `../.env` — valid config file, no change.
- Names consistent: `DefaultPath/Values/Save/Defaults/Providers/DefaultProvider`, `configtui.Run/Mask`, `applyDefaults`.
- Risk: huh v2 API differs from v1 docs → check `go doc charm.land/huh/v2` before writing C4.
