---
allowed-tools: Bash(cp:*), Bash(ln:*), Bash(mkdir:*), Bash(ls:*), Bash(rm:*), Bash(readlink:*), Bash(date:*), Bash(systemctl --user restart:*), Bash(systemctl --user status:*)
description: Promote staging build to current (make it live) and restart daemons
---

## Context

- Current version: !`readlink builds/current`
- Staging contents: !`ls -la builds/staging/ 2>/dev/null || echo "Empty - run /build first"`
- Available versions: !`ls -d builds/2* 2>/dev/null | sort -r | head -5 || echo "None"`

## Your task

Promote the staging build to become the current (live) version. This WILL restart the daemons.

Steps:
1. Verify staging has binaries (if empty, tell user to run `/build` first)
2. Create a timestamped version directory: `builds/$(date +%Y%m%d-%H%M%S)/`
3. Copy all binaries from `builds/staging/` to the new timestamped directory
4. Remove the old `builds/current` symlink
5. Create new symlink: `ln -s <timestamp> builds/current`
6. Restart the daemons: `systemctl --user restart transcribe-daemon recording-daemon hotkey-daemon`
7. Show daemon status to confirm they're running
8. Report the new version that is now live

Important:
- This WILL restart daemons and briefly interrupt dictation
- The old version is preserved for rollback
