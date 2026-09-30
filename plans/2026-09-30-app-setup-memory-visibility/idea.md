# Configure OpenCode and show actual runtime state

**Date:** 2026-09-30
**Status:** done

The user wants Configure/Repair OpenCode inside Lobocode, using the running endpoint, key and actual Q6/Q8 model.
Preserve other providers and custom settings; make using Lobocode by default an explicit checkbox before writing.
Show measured model memory during startup and Ready, with separate labels for Mac available memory and cloud device VRAM.
Show Reading prompt, Generating, Idle or Unavailable instead of misleading zero rates during active work.
Make Lobocode appear in the macOS Dock and Cmd+Tab, with working reopen and native windows.
Fix local Stop so it never queries or deletes cloud resources; verify fixes with small direct scripted requests, not OpenCode.
Keep every view compact and free of scrolling; keep credentials and prompt content out of status and reports.
Draft one feature spec now. Existing work and image jobs remain paused; implementation follows the memory feature's tested, merged baseline.
