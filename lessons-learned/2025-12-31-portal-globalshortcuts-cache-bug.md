# XDG Desktop Portal GlobalShortcuts Cache Bug (Part 2)

**Date:** 2025-12-31
**Builds on:** 2025-12-30-xdg-desktop-portal-globalshortcuts.md
**Issue:** hotkey-daemon still fails at boot despite previous fixes

## Problem

After implementing the fixes from 2025-12-30 (Requires=, After=, ExecStartPre wait loop), hotkey-daemon continued to fail at boot:

```
Dec 31 06:25:59 sh[2100]: GlobalShortcuts not available after 30s
Dec 31 06:26:32 sh[3093]: GlobalShortcuts not available after 30s
... (5 retry cycles)
```

The 30-second polling loop kept failing even though both portal services were running.

## Root Cause: Portal Interface Caching

The previous analysis missed a critical behavior:

1. `xdg-desktop-portal` starts and queries all backends for available interfaces
2. `xdg-desktop-portal-hyprland` wasn't ready yet (WAYLAND_DISPLAY condition failed initially)
3. Main portal **caches** the interface list - GlobalShortcuts not found
4. Hyprland portal starts 2 seconds later via D-Bus activation
5. **Main portal never re-queries backends** - cache is stale
6. Our polling loop keeps checking the same stale cache

Evidence from logs:
```
06:25:27 - Portal was skipped: ConditionEnvironment=WAYLAND_DISPLAY
06:25:29 - Hyprland portal finally started via D-Bus activation
06:25:29 - Main portal started but already cached (no GlobalShortcuts)
```

The hyprland backend **does** expose GlobalShortcuts:
```bash
busctl --user introspect org.freedesktop.impl.portal.desktop.hyprland \
  /org/freedesktop/portal/desktop | grep GlobalShortcuts
# org.freedesktop.impl.portal.GlobalShortcuts  interface
```

But the main portal doesn't expose it on the client-facing path because it cached the empty state.

## What Didn't Work

1. **Longer polling timeout (30s)** - Useless; polls stale cache forever
2. **Requires= and After= ordering** - Services start but portal caches before hyprland is ready
3. **Drop-in with WantedBy=graphical-session.target** - Had syntax error (WantedBy in [Unit] is invalid), and still didn't guarantee timing
4. **Pinging the portal before polling** - Doesn't force interface re-discovery

## Solution: Force Portal Restart

The only way to clear the stale cache is to restart the main portal after the hyprland backend is running:

```ini
[Service]
# Force portal restart to ensure GlobalShortcuts interface is discovered
# (fixes race condition where main portal caches stale backend state at boot)
ExecStartPre=/bin/sh -c 'systemctl --user restart xdg-desktop-portal-hyprland xdg-desktop-portal && sleep 1'
# Verify GlobalShortcuts is available (should be immediate after restart)
ExecStartPre=/bin/sh -c 'for i in $(seq 1 10); do busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop 2>/dev/null | grep -q GlobalShortcuts && exit 0; sleep 1; done; echo "GlobalShortcuts not available"; exit 1'
```

This works because:
1. By the time hotkey-daemon starts, hyprland portal is definitely running (Requires=)
2. Restarting both portals forces fresh backend discovery
3. Main portal now finds GlobalShortcuts on the hyprland backend
4. Short verification loop (10s) catches edge cases

## Debugging Commands

```bash
# Check what interfaces the MAIN portal exposes (client-facing)
busctl --user introspect org.freedesktop.portal.Desktop \
  /org/freedesktop/portal/desktop | grep -i shortcut

# Check what interfaces the HYPRLAND backend exposes (implementation)
busctl --user introspect org.freedesktop.impl.portal.desktop.hyprland \
  /org/freedesktop/portal/desktop | grep -i shortcut

# If first is empty but second has GlobalShortcuts, you have the cache bug
# Fix: systemctl --user restart xdg-desktop-portal-hyprland xdg-desktop-portal

# Check portal routing configuration
cat /usr/share/xdg-desktop-portal/hyprland-portals.conf
# Should show: default=hyprland;gtk
```

## Key Takeaways

1. **xdg-desktop-portal caches backend interfaces** - It queries once at startup and never re-checks
2. **Polling a stale portal is futile** - The interface list won't change without a restart
3. **Check BOTH portal paths when debugging** - `org.freedesktop.portal.Desktop` (client) vs `org.freedesktop.impl.portal.desktop.hyprland` (backend)
4. **Restart is the nuclear option** - Sometimes the only fix for race conditions in D-Bus-activated services
5. **ConditionEnvironment= causes silent skips** - Check journal for "was skipped because of an unmet condition check"

## Trade-offs of This Fix

**Pros:**
- Guarantees fresh portal state
- Self-contained in hotkey-daemon.service
- No manual intervention after reboot

**Cons:**
- Adds ~2s to hotkey-daemon startup
- Briefly disrupts other portal users (file chooser dialogs might reset)
- Masks the underlying race condition rather than fixing it properly

## Alternative Approaches Not Tried

1. **Upstream fix** - Report to xdg-desktop-portal that it should re-query backends when new ones register
2. **Socket activation for hotkey-daemon** - Start only when portal is fully ready (complex)
3. **Hyprland-specific startup script** - Add portal restart to hyprland.conf exec-once (moves problem elsewhere)

## Files Modified

- `~/.config/systemd/user/hotkey-daemon.service` - Added portal restart in ExecStartPre
