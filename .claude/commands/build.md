---
allowed-tools: Bash(cargo build:*), Bash(cargo check:*), Bash(cp:*), Bash(mkdir:*), Bash(ls:*)
description: Build release binaries to staging directory (safe, doesn't affect running daemons)
---

## Context

- Current staging contents: !`ls -la builds/staging/ 2>/dev/null || echo "Empty"`
- Current version: !`readlink builds/current`

## Your task

Build the release binaries and copy them to the staging directory. This does NOT affect running daemons.

Steps:
1. Run `cargo build --release` to compile all binaries
2. Copy the binaries from `target/release/` to `builds/staging/`:
   - transcribe
   - transcribe-daemon
   - transcribe-client
   - recording-daemon
   - hotkey-daemon
3. Report success and show what's in staging

Important:
- Do NOT restart any daemons
- Do NOT promote to current
- Just build and stage - the user will test and promote separately
