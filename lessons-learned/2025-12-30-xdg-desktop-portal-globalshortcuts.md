# XDG Desktop Portal GlobalShortcuts Boot Race Condition

**Date:** 2025-12-30
**Issue:** hotkey-daemon fails to start at boot with "No such interface org.freedesktop.portal.GlobalShortcuts"

## Problem

After switching the hotkey-daemon from evdev-based keyboard grabbing to the Hyprland GlobalShortcuts D-Bus portal, the service would fail repeatedly at boot:

```
Error: MethodError("org.freedesktop.DBus.Error.UnknownMethod",
       "No such interface org.freedesktop.portal.GlobalShortcuts on object at path /org/freedesktop/portal/desktop")
```

The hotkey-daemon was restarting hundreds of times (restart counter > 200) because the GlobalShortcuts interface wasn't available.

## Root Cause

The XDG desktop portal system has multiple components:

1. **xdg-desktop-portal** - Main portal service (`org.freedesktop.portal.Desktop`)
2. **xdg-desktop-portal-hyprland** - Hyprland-specific implementation providing Screenshot, ScreenCast, and **GlobalShortcuts**
3. **xdg-desktop-portal-gtk** - GTK fallback for other portal interfaces

Both portal services are `Type=dbus` which means they're D-Bus activated (start on demand when their bus name is requested). The problem:

1. At boot, `xdg-desktop-portal` starts and registers as `org.freedesktop.portal.Desktop`
2. `xdg-desktop-portal-hyprland` is NOT started yet (no D-Bus activation triggered it)
3. hotkey-daemon starts and requests `org.freedesktop.portal.GlobalShortcuts`
4. The main portal doesn't have this interface because the hyprland backend isn't loaded
5. Error returned, hotkey-daemon crashes, restarts, repeat forever

## Solution

Three-part fix to ensure proper startup order:

### 1. Enable xdg-desktop-portal-hyprland for graphical session

Create drop-in to make the service start with the graphical session:

```ini
# ~/.config/systemd/user/xdg-desktop-portal-hyprland.service.d/autostart.conf
[Unit]
WantedBy=graphical-session.target

[Install]
WantedBy=graphical-session.target
```

Then enable it:
```bash
systemctl --user daemon-reload
systemctl --user enable xdg-desktop-portal-hyprland.service
# Creates symlink in graphical-session.target.wants/
```

### 2. Strong dependency in hotkey-daemon.service

Use `Requires=` instead of `Wants=` for hard dependency:

```ini
[Unit]
After=graphical-session.target xdg-desktop-portal-hyprland.service xdg-desktop-portal.service
Requires=xdg-desktop-portal-hyprland.service xdg-desktop-portal.service
```

### 3. Wait for GlobalShortcuts interface availability

Even with proper ordering, there's a brief window where the portal is running but hasn't registered all interfaces. Add an ExecStartPre that waits:

```ini
[Service]
ExecStartPre=/bin/sh -c 'for i in 1 2 3 4 5; do busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop 2>/dev/null | grep -q GlobalShortcuts && exit 0; sleep 1; done; exit 1'
```

This polls for up to 5 seconds for the GlobalShortcuts interface to appear before starting the daemon.

## Debugging Commands

```bash
# Check if portal services are running
systemctl --user status xdg-desktop-portal xdg-desktop-portal-hyprland

# List available portal interfaces
busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop | grep interface

# Check which portals are registered
busctl --user list | grep portal

# Check portal configuration
cat /usr/share/xdg-desktop-portal/portals/hyprland.portal
cat ~/.config/xdg-desktop-portal/hyprland-portals.conf
```

## Key Takeaways

1. **D-Bus activated services are lazy** - They don't start until something requests their bus name
2. **Portal backends register asynchronously** - Even after xdg-desktop-portal starts, backends like hyprland need time to register their interfaces
3. **systemd After= isn't enough** - The service may be "started" but not fully functional; use ExecStartPre to verify interface availability
4. **Drop-ins can add [Install] sections** - This lets you make static/D-Bus services start with targets like graphical-session.target

## Files Modified

- `~/.config/systemd/user/hotkey-daemon.service` - Added Requires=, ExecStartPre wait loop
- `~/.config/systemd/user/xdg-desktop-portal-hyprland.service.d/autostart.conf` - Created to enable service at graphical session
