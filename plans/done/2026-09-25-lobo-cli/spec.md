# lobo as an installable binary: config TUI, defaults, Homebrew prep

**Date:** 2026-09-25
**Scope:** ~/dev/lobotomized-ai (+ ~/dev/homebrew-tap, ~/dev/rival remotes)
**Status:** done

## TL;DR

**P1 — system config file + `make install`**
- What: `lobo` reads `~/.config/lobo/config.env` (dotenv, 0600) instead of the repo `.env`. `make install` builds with the version baked in and copies `lobo` to `~/.local/bin`. `make up/down/status/logs/smoke/gen-api-key` are removed.
- Why: today `lobo` only works from inside the repo (`--env .env`, `cmd/lobo/main.go:39`).
- You do: nothing. I copy the repo `.env` into the new config file once (same keys, same format).
- Does NOT: read keys from the shell environment (still banned), or change the pod side.

**P2 — `lobo config` TUI + defaults**
- What: `lobo config` opens a bubbletea form (huh v2 on bubbletea v2): provider keys, access keys, and defaults (default provider, min MB/s, model, ctx, idle min, max hours, RunPod cloud, Vast max $/h). It shows the file path and says you can edit it by hand. `lobo config path` / `lobo config show` (keys masked) for scripts.
- Why: no way to set keys or defaults without editing `.env` by hand.
- You do: nothing.
- Does NOT: store keys in the macOS keychain.

**P3 — defaults + provider pick in `up`, better help**
- What: flags > config defaults > built-in defaults. With both RunPod and Vast keys, `up` uses `LOBO_PROVIDER` (default runpod); `--provider` overrides. RunPod key becomes optional (at least one provider key required). `lobo` with no args prints a styled help with grouped commands and a "run `lobo config` first" hint when no config exists. `release` is hidden (dev-only).
- Why: you asked for a nice binary with defaults you set once.
- You do: nothing.
- Does NOT: auto-fallback between providers.

**P4 — Homebrew prep (no release)**
- What: `.goreleaser.yaml` (darwin/linux × amd64/arm64, formula `lobo.rb` → `1905/homebrew-tap`, same pattern as rival) + `.github/workflows/release.yml` on `v*` tags. Verified only with `goreleaser check` and `goreleaser release --snapshot --clean` locally. Git remotes of `~/dev/homebrew-tap` and `~/dev/rival` switch from the `github-1f47e` SSH alias to `https://github.com/1905/…`.
- Why: brew install later with one tag push.
- You do (later, before the first release): make `1905/lobotomized-ai` public (or pick a public binaries repo — brew can't download private release assets), add the `HOMEBREW_TAP_TOKEN` secret to the repo, fix the domain. Then push tag `v0.1.0`.
- Does NOT: push a tag, publish a release, or touch the tap repo contents.

## Problem(s)

1. Config lives only in the repo `.env`. `--env` defaults to `.env` in the current dir (`cmd/lobo/main.go:39`), `LoadLaptop` reads that path only (`internal/config/laptop.go:45`). Run `lobo` anywhere else and it fails.
2. Daily use goes through `make up` etc. (`Makefile:25-45`), which rebuilds every time and needs the repo checkout.
3. No defaults. Every `up` flag default is hard-coded (`cmd/lobo/main.go:201-212`); only `LOBO_MIN_MBPS` and `LOBO_VAST_MAX_DPH` come from config. `--provider` defaults to runpod even when only a Vast key exists, and `RUNPOD_API_KEY` is `validate:"required"` (`internal/config/laptop.go:17`), so a Vast-only user can't run at all.
4. No distribution. No goreleaser config, no tap formula. Tap and rival remotes still use the old `github-1f47e` SSH alias (`git -C ~/dev/homebrew-tap remote -v`).

## Goals

1. `lobo` works from any directory with one config file. (1)
2. `make install` puts a versioned `lobo` on PATH. (2)
3. `lobo config` sets every key and default in a TUI and names the file. (1, 3)
4. `up` takes defaults from config; flags override; Vast-only setup works. (3)
5. One `git tag v… && git push --tags` would publish a brew formula — prepared, not run. (4)

## Non-goals

- Keychain storage, encrypted config.
- Reading keys from the OS environment.
- Publishing a release, making the repo public, domain work.
- Changing the agent, pod bootstrap, or the agent release flow (`lobo release` stays, hidden).

## Config file

Path: `$XDG_CONFIG_HOME/lobo/config.env`, else `~/.config/lobo/config.env`. Override: `--config PATH` (persistent flag; hidden alias `--env` kept for the Makefile and old habits). Dir 0700, file 0600; `lobo` warns if the file is group/world readable.

Format = dotenv, same key names as today's `.env`, so the repo `.env` is a valid config file. `lobo config` writes a header comment and groups:

```
# lobo config — edit by hand or run `lobo config`.
# Keys here never go into a pod image or release. Flags on `lobo up` override the defaults below.

# providers (at least one)
RUNPOD_API_KEY=…
VASTAI_API_KEY=…
# access
LOBO_DOMAIN=lobo.example.com
LOBO_API_KEY=…
CF_TUNNEL_TOKEN=…
LOBO_BUCKET_URL=https://pub-….r2.dev
# defaults for `lobo up`
LOBO_PROVIDER=runpod        # runpod | vast (used when both keys are set)
LOBO_MIN_MBPS=100
LOBO_MODEL=q8               # q8 | q6
LOBO_CTX=0                  # 0 = release default (65536)
LOBO_IDLE_MIN=0             # 0 = release default (30)
LOBO_MAX_HOURS=0            # 0 = release default (12)
LOBO_CLOUD=secure           # runpod: secure | community first
LOBO_VAST_MAX_DPH=1.20
# advanced (edit by hand): LOBO_MODEL_SOURCE, LOBO_MODEL_SSH_KEY_FILE, LOBO_MODEL_SSH_HOSTKEY,
# LOBO_FEESH_HTTP_URL, R2_* (only `lobo release` needs these)
```

Writing: `lobo config` rewrites known keys in place and keeps unknown keys and their lines (no silent loss of hand edits). Write = temp file + rename, 0600.

## `lobo config` TUI

huh v2 form (charm.land/huh/v2, built on bubbletea v2 — already our TUI stack), lipgloss theme matching the `up` TUI. Header shows the file path and "you can edit this file by hand".

Groups (one page each):
1. Providers — RunPod key, Vast key (password inputs; empty = unchanged when a value exists; "clear" by typing `-`). Validation: at least one set.
2. Access — domain, LOBO API key (select: keep / generate new — reuses `gen-api-key` code), CF tunnel token, bucket URL.
3. Defaults — default provider (select; only shown when both keys set), min MB/s, model, ctx, idle min, max hours, RunPod cloud, Vast max $/h. Inputs validated as numbers/ranges.
4. Confirm — summary with masked keys → save.

Non-TTY: `lobo config` prints the path and `lobo config show` output instead of opening the form. Subcommands: `lobo config path`, `lobo config show` (masked `abcd…wxyz`).

## Defaults resolution in `up`

```
flag set by user  →  config key  →  built-in default
--provider           LOBO_PROVIDER   runpod if its key exists, else vast
--min-mbps           LOBO_MIN_MBPS   100
--q6                 LOBO_MODEL      release default (q8)
--ctx                LOBO_CTX        release default
--idle-min           LOBO_IDLE_MIN   release default
--max-life           LOBO_MAX_HOURS  release default
--cloud              LOBO_CLOUD      secure
```

"Flag set" = `cmd.Flags().Changed(name)`. Provider without a key → error "no VASTAI_API_KEY in <path> — run `lobo config`".

## Help

`lobo` (no args) and `lobo help`: lipgloss-styled help — one line of what lobo is, groups **Run** (`up`, `down`, `status`, `logs`, `test`), **Setup** (`config`, `gen-api-key`, `version`), examples (`lobo up`, `lobo up --provider vast`, `lobo up --q6 --min-mbps 200`). If the config file is missing: a first line "no config yet — run `lobo config`". Styling off when stdout is not a TTY or `NO_COLOR` is set.

## Build, install, release prep

- `version` via ldflags `-X main.version=$(git describe --tags --always --dirty)`; `lobo version` also prints commit and build date.
- Makefile: `build`, `build-agent`, `install` (`PREFIX ?= $(HOME)/.local` → `$(PREFIX)/bin/lobo`), `test`, `lint`, `e2e`, `release` (`bin/lobo release --config .env`). Removed: `up down status logs smoke gen-api-key`.
- `.goreleaser.yaml` at repo root: build `./cmd/lobo` only, `CGO_ENABLED=0`, darwin/linux × amd64/arm64, archive `lobo_{{.Os}}_{{.Arch}}.tar.gz`, `brews:` → `1905/homebrew-tap` branch master, formula at root (`lobo.rb`), token `HOMEBREW_TAP_TOKEN`, test `lobo version`.
- `.github/workflows/release.yml`: on `v*` tag, goreleaser v2 (copy of rival's, workdir = root).
- Agent release tags (`2026.09.25-N`) live in R2, not git tags — no clash with `v*`.

## File-level changes

| File | Change |
|---|---|
| `internal/config/path.go` (new) | `DefaultPath()` (XDG → ~/.config/lobo/config.env), `Exists`, perm check. |
| `internal/config/laptop.go` | `RUNPOD_API_KEY` optional + "at least one provider key" check; new default keys (`LOBO_PROVIDER, LOBO_MODEL, LOBO_CTX, LOBO_IDLE_MIN, LOBO_MAX_HOURS, LOBO_CLOUD`) parsed + validated. |
| `internal/config/write.go` (new) | `Save(path, values)`: update known keys in place, keep unknown lines, header on new file, atomic 0600. |
| `internal/configtui/` (new) | huh form model, theme, masking; pure `toValues/fromValues` for tests. |
| `cmd/lobo/main.go` | `--config` (default `DefaultPath()`), hidden `--env`; `up` defaults from config via `Changed`; provider pick; `release` hidden. |
| `cmd/lobo/config.go` (new) | `config`, `config path`, `config show`. |
| `cmd/lobo/help.go` (new) | styled root help/usage template. |
| `cmd/lobo/version` | commit + date ldflags. |
| `Makefile` | as above. |
| `e2e/e2e_test.go` | `LOBO_CONFIG`-style path: keep `../.env`. No change unless load API changes. |
| `.goreleaser.yaml`, `.github/workflows/release.yml` (new) | as above. |
| `README.md`, `.env.example` | install/config/brew sections; `.env.example` gets the defaults block. |
| `~/dev/homebrew-tap`, `~/dev/rival` | `git remote set-url origin https://github.com/1905/<repo>.git`; `git fetch` to verify. |

## Tests

- config: `DefaultPath` with/without `XDG_CONFIG_HOME`; Vast-only config valid; no provider key → error; bad `LOBO_MODEL/LOBO_CLOUD/LOBO_PROVIDER` → error.
- write: round-trip keeps unknown keys, comments, order; new file gets header; file mode 0600; atomic (no partial file on error).
- up defaults: table test of flag/config/built-in resolution incl. provider pick (runpod only, vast only, both + LOBO_PROVIDER, flag override).
- configtui: `fromValues→toValues` round trip; empty password field keeps old value; `-` clears; numeric validators.
- help: golden of root help (no color) with and without a config file.
- Manual: `make install` → `lobo` from `/tmp` prints help; `lobo config` in a real terminal (tmux capture); `goreleaser check`; `goreleaser release --snapshot --clean` builds 4 archives + `lobo.rb`.

## Failure modes & decisions

| Failure | Behaviour |
|---|---|
| No config file | help hint; `up/down/status` error "no config at <path> — run `lobo config`". |
| Config world-readable | warn once on stderr with `chmod 600 <path>`. |
| Hand-edited file with unknown keys | kept on save. |
| Both provider keys, no `LOBO_PROVIDER` | runpod (today's default). |
| Non-TTY `lobo config` | print path + masked values; no form. |
| `~/.local/bin` not on PATH | `make install` prints a PATH hint. |
| Brew formula points to a private repo's assets | Known blocker; listed in TL;DR P4 for the user. No release until fixed. |

## Out of scope

- First real release, tag push, repo visibility, domain.
- Keychain, Windows builds.
- Interactive provider picker inside `up`.

## Rollout

- P1 config path + `make install` (one commit).
- P2 `lobo config` TUI + writer.
- P3 defaults in `up`, provider pick, help.
- P4 goreleaser + workflow + remotes; snapshot verified.
- End: one `/rival-astra` review of the branch → fixes → merge → notify.

## As-built notes

- Extra CLI hooks for the macOS app: `config show --json`, `config set` (args or `--stdin` JSON), `config get`, `down --json`.
- Astra fixes: bad launch defaults never block `down` and flags override them; dotenv writes escape `$`/newlines; `config show` masks unknown keys; the save summary refreshes.
- No LICENSE in the repo: the formula carries no license line.
