---
allowed-tools: Bash(ls:*), Bash(ln:*), Bash(rm:*), Bash(readlink:*), Bash(systemctl --user restart:*), Bash(systemctl --user status:*)
description: Rollback to a previous build version
---

## Context

- Current version: !`readlink builds/current`
- Available versions: !`ls -d builds/2* 2>/dev/null | sort -r || echo "None"`

## Your task

Rollback to a previous version. By default, rollback to the version before the current one.

Steps:
1. List available versions (sorted newest first)
2. Identify the current version and the previous version
3. If there's only one version, report that rollback is not possible
4. Remove the `builds/current` symlink
5. Create new symlink pointing to the previous version: `ln -s <previous> builds/current`
6. Restart the daemons: `systemctl --user restart transcribe-daemon recording-daemon hotkey-daemon`
7. Show daemon status to confirm they're running
8. Report which version is now active

If the user specified a specific version to rollback to (as an argument), use that instead of the previous version.
