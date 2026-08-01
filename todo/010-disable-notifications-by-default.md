# Disable notification alerts by default

## Status

Feature request from regular use; current default notifications are noisy.

## Problem

PlasmaTop has per-category notification switches in `config.toml`, but several categories default to enabled and there is no single user-facing master switch. Notification emission should be globally toggleable and disabled by default for fresh configurations.

## Current behavior

Disk usage, SMART, disk temperature, and battery notification categories default to enabled. CPU temperature, NVIDIA temperature, load average, and server checks default to disabled. Notification settings belong to the daemon configuration rather than the applet KConfig page.

## Relevant files

- `config/config.toml`
- `src/config/mod.rs`
- `src/notify.rs`
- `src/daemon.rs`
- `docs/DESIGN.md`

## Handoff

1. Add one master notification gate with a false default while retaining per-category preferences for users who opt in.
2. Define upgrade behavior so changing defaults does not unexpectedly overwrite explicit existing configuration.
3. Ensure disabling notifications also avoids notification-only sensor work where no displayed item needs it.
4. Document how to enable notifications and individual categories.
5. Test reload, last-good config behavior, latch state, and re-enabling after alerts were suppressed.

## Done when

- Fresh installations emit no desktop notifications until explicitly enabled.
- One setting disables all notification delivery without erasing category choices.
- Disabled notification-only capabilities do not trigger unnecessary collection.
- Config, notification, daemon lifecycle, and full repository tests pass.

