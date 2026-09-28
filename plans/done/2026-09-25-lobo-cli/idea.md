# lobo as an installable binary

**Date:** 2026-09-25
**Status:** done

User ask (verbatim gist): no more `make up`. Only `make install` (build + install into the system), then `lobo up`.
`lobo` alone prints help. `lobo config` = a good bubbletea TUI to enter API keys, and it says where the
config file lives so the user can edit it by hand. `lobo config` also sets defaults (speed cap, provider when
both RunPod and Vast keys are set; flags still override). Release like rival: goreleaser → brew tap `1905/homebrew-tap`.
Update the git remotes there (account moved `1F47E` → `1905`). Prepare only — no release yet (domain etc. not ready).
Starts after feat/vast is merged, on its own branch. One Astra review at the end. Full auto.
