# User-local installation plan

## Status

Proposed. This plan adds a supported installation path for immutable/Atomic
desktops such as Bazzite without changing native distro-package installation.

## Goal

Make this work without `sudo`, writes under `/usr`, package layering, or a
reboot:

```bash
./install.sh --user
```

The result must provide the same daemon, CLI, Plasma applet, configuration,
upgrade, and uninstall behavior as the system-wide install. A checkout may be
deleted after installation.

## Non-goals

- Do not replace the existing system-wide installer or AUR package layout.
- Do not add Flatpak, Toolbox, Distrobox, Homebrew, RPM, or `rpm-ostree`
  packaging. Those environments may build the binary, but are not runtime
  layouts.
- Do not run the daemon inside a container.
- Do not change sensor, render, runtime-file, or configuration behavior.
- Do not make the installed app depend on the source checkout.
- Do not add a second daemon, service name, applet id, or config tree.
- Do not edit `plasma-org.kde.plasma.desktop-appletsrc` directly.

## User-facing contract

### Commands

```bash
./install.sh              # existing system-wide install
./install.sh --user       # new user-local install
./uninstall.sh            # existing system-wide uninstall
./uninstall.sh --user     # new user-local uninstall
```

Unknown arguments and incompatible combinations fail before building or
writing files. `DESTDIR` remains system-package staging only; reject
`DESTDIR` with `--user` rather than inventing unclear path composition.

`PIROSTATS_BINARY=/absolute/path/to/pirostats` remains the supported way to
skip a local Cargo build. This lets an Atomic host build in Distrobox/Toolbox
and install the resulting host-compatible binary from the host shell.

### User-local layout

Resolve `XDG_DATA_HOME` once during installation, defaulting to
`$HOME/.local/share`. Keep the executable launcher in the conventional
`$HOME/.local/bin`; Linux has no standard `XDG_BIN_HOME`.

```text
$HOME/.local/bin/pirostats
$XDG_DATA_HOME/pirostats/
  pirostats
  config/
  style/
  lang/
$XDG_DATA_HOME/systemd/user/pirostats.service
$XDG_DATA_HOME/plasma/plasmoids/com.github.lucazade.pirostats/
$XDG_DATA_HOME/icons/hicolor/scalable/apps/pirostats.svg
$XDG_DATA_HOME/licenses/pirostats/{LICENSE,NOTICE}
```

The applet directory is shown for ownership/testing purposes. Let
`kpackagetool6` choose and manage its actual user-local KDE data location; do
not duplicate its installation logic with `cp` in live mode.

Existing writable state stays unchanged:

```text
$XDG_CONFIG_HOME/pirostats/       user config and style overrides
$XDG_CACHE_HOME/pirostats/        cached geometry
$XDG_RUNTIME_DIR/pirostats/       live daemon/applet protocol
```

### Install lifecycle

1. Parse mode and validate requirements before writing anything.
2. Build the locked release binary with `nvml`, unless
   `PIROSTATS_BINARY` was supplied.
3. Stage the complete code tree in a sibling temporary directory.
4. Replace `$XDG_DATA_HOME/pirostats` only after build/staging succeeds.
5. Install the launcher, unit, icon, and licenses.
6. Install or upgrade the applet in user scope.
7. Run `systemctl --user daemon-reload`.
8. Enable and restart `pirostats.service` as the invoking user.
9. Refresh KDE's service cache when `kbuildsycoca6` exists.
10. Restart Plasma only when an applet upgrade requires it, preserving current
    installer behavior.

A failed build must leave the previous install running. A failed activation
after file replacement must return non-zero and print the exact recovery
commands; it must not silently claim success.

### Upgrade and uninstall

- Re-running `./install.sh --user` upgrades in place and removes stale shipped
  files.
- User config under `$XDG_CONFIG_HOME/pirostats` always survives.
- User uninstall disables/stops the service, removes only user-local files
  owned by PiroStats, removes the user-scoped applet, reloads systemd, and
  removes PiroStats runtime/cache files as the current uninstaller does.
- User uninstall never calls `sudo`, passes `--global`, or removes any `/usr`
  path.
- If a system-wide PiroStats package also exists, user uninstall reveals it; it
  does not remove or modify it.

## Design decisions

### One installer with an explicit mode

Extend `install.sh` and `uninstall.sh` with `--user`. Do not create independent
`install-user.sh` scripts: two full installers would immediately drift in build
flags, applet handling, cleanup, and messages.

Factor only shared operations that have two real callers: argument parsing,
binary build/validation, applet install/upgrade, KDE cache refresh, and service
activation. Keep mode-specific file manifests explicit and readable.

### Preserve runtime asset resolution

The installed launcher remains the boundary that exports
`PIROSTATS_CODE_ROOT`. The user launcher must point it at the resolved
`$XDG_DATA_HOME/pirostats` tree, then execute that tree's binary. No Rust config
or asset-resolution change is needed; `rust/src/config/assets.rs` already owns
this contract.

Generate the launcher at install time with safely shell-quoted absolute paths.
Do not rely on `XDG_DATA_HOME` being exported into the Plasma or systemd user
environment after login.

### Keep service discovery user-native

Install the unit under `$XDG_DATA_HOME/systemd/user`, which is part of systemd's
documented user-unit search path. This avoids treating
`$XDG_CONFIG_HOME/systemd/user` as package-owned and leaves that higher-priority
directory available for normal user overrides.

Use `%h/.local/bin/pirostats daemon` in the user unit. Do not depend on the
service manager's `PATH`. Keep all other unit semantics equal to
`service/pirostats.service` (`PartOf`, restart policy, and install target).

Add `service/pirostats-user.service` rather than templating the native package
unit. One path-only variant keeps AUR/system package consumers unchanged. Add a
test that normalizes `ExecStart` and asserts both units otherwise match, so the
small duplication cannot drift.

### Make applet action commands installation-neutral

Current defaults in `plasmoid/package/contents/config/main.xml` hardcode
`/usr/bin/pirostats` for click and wheel actions. Panel/tooltip reads do not have
this issue because QML derives the runtime directory independently.

For user install, stage a temporary applet package and replace only those three
defaults with shell-expanded user paths:

```text
$HOME/.local/bin/pirostats click
$HOME/.local/bin/pirostats page prev
$HOME/.local/bin/pirostats page next
```

Use literal `$HOME` in the command strings so paths containing spaces remain
shell-tokenizable only when quoted; the generated XML values must therefore
quote the executable path. Prefer a small Python or shell substitution already
used by `tools/qml_verify.sh`; do not mutate the checkout's canonical applet.
System installation continues to use `/usr/bin/pirostats` defaults unchanged.

Before implementation, verify on Plasma 6 whether these hidden KConfig entries
store defaults in existing widget instances. The required result is:

- fresh user install executes the user launcher;
- user-local upgrade keeps working;
- migration from a previously instantiated system applet either adopts the new
  defaults or emits a clear remove/re-add instruction.

Do not rewrite Plasma's applet configuration database to force migration. If
KConfig persists the old absolute values, document remove/re-add as the safe
migration boundary.

### Define system/user coexistence instead of guessing

Same service name and applet id mean system and user installations are
overrides, not two independent instances:

- user systemd unit has higher lookup priority than `/usr/lib/systemd/user`;
- user KDE package should shadow the global package with the same id;
- only one daemon should run because both units share one name.

Test these claims on Plasma 6. If `kpackagetool6` refuses a user package when a
global package exists, fail before activation with instructions to remove the
manual system install first. Never invoke the system uninstaller automatically.

## Implementation phases

### Phase 1: freeze paths and mode parsing

Files:

- `install.sh`
- `uninstall.sh`
- `tools/p6_package_test.sh`

Work:

1. Add strict zero-or-one argument parsing for `--user` and `--help`.
2. Compute mode before `SUDO`, root, or destination variables.
3. In user mode, require non-empty absolute `HOME`; resolve
   `XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}` and reject relative
   values.
4. Define explicit per-mode paths and applet scope; avoid conditionals scattered
   around raw path literals.
5. Reject `DESTDIR` plus `--user`.
6. Preserve no-argument behavior byte-for-byte where practical.

Gate:

- `bash -n install.sh uninstall.sh` passes.
- Unknown flags and `DESTDIR --user` fail without filesystem changes.
- Existing `DESTDIR` native package test remains green.

### Phase 2: install user-owned files

Files:

- `install.sh`
- `packaging/pirostats-user-launcher` or generated launcher logic in
  `install.sh` (choose generated logic if it stays shorter)
- `service/pirostats-user.service`

Work:

1. Reuse current locked Cargo build and `PIROSTATS_BINARY` validation.
2. Stage binary plus `config/`, `style/`, and `lang/` under the user data root.
3. Generate/install launcher mode `0755`; install data and units with
   non-world-writable permissions.
4. Install icon and license files under user data.
5. Replace old data tree only after staging is complete; ensure interrupted
   cleanup removes temporary trees.
6. Run the installed launcher with `list-items` before service activation. This
   proves executable and shipped assets resolve independently of checkout.

Gate:

- User install contains no symlink into the checkout.
- Moving the checkout does not break `pirostats list-items`.
- Failed build/invalid `PIROSTATS_BINARY` preserves an existing install.
- Reinstall removes an injected stale shipped file.

### Phase 3: install applet and activate service

Files:

- `install.sh`
- temporary applet staging logic
- `service/pirostats-user.service`

Work:

1. Copy the applet to a temporary directory and patch only action defaults.
2. Use user-scoped `kpackagetool6 --install`/`--upgrade`; never pass `--global`
   or `sudo` in user mode.
3. Install the user unit, reload systemd, enable it, and restart it.
4. Verify `systemctl --user is-active pirostats` after restart; report journal
   command on failure.
5. Preserve first-install guidance and upgrade Plasma reload behavior.
6. Ensure all temporary applet/build staging cleanup runs through a trap.

Gate:

- Fake-tool integration test records no `sudo` or `--global` call.
- Service command resolves to the installed user launcher.
- Click and both wheel directions invoke that launcher.
- Daemon publishes panel/tooltip files in the normal runtime directory.

### Phase 4: user-local uninstall and migration behavior

Files:

- `uninstall.sh`

Work:

1. Select the same path model as install; do not infer mode from what happens to
   exist on disk.
2. Disable/stop user service before removing its launcher or binary.
3. Remove user-scoped applet through `kpackagetool6` without `--global`.
4. Remove exact owned files/directories, then prune only empty parent
   directories created for PiroStats.
5. Reload systemd and remove current runtime/cache artifacts.
6. Preserve `$XDG_CONFIG_HOME/pirostats` and print that fact.
7. Make repeat uninstall successful and harmless.

Gate:

- Install, uninstall, uninstall again all succeed.
- User config fixture survives exactly.
- No system path changes before/after user uninstall.
- Coexisting system install remains usable after user uninstall.

### Phase 5: tests and immutable-host documentation

Files:

- `tools/p6_package_test.sh`
- optional focused `tools/user_install_test.sh` if adding the cases would make
  `p6_package_test.sh` unreadable
- `tools/qml_verify.sh`
- `README.md`
- `docs/DEVELOPMENT.md`

Work:

1. Add disposable fake-`HOME`/`XDG_DATA_HOME` install, repeat-upgrade, and
   uninstall coverage using `PIROSTATS_BINARY`.
2. Put fake `systemctl`, `kpackagetool6`, `kbuildsycoca6`, `kstart`, and `sudo`
   commands first in `PATH`; record argv and fail if user mode invokes `sudo` or
   `--global`.
3. Simulate applet install/upgrade sufficiently to assert patched XML defaults
   and package contents.
4. Keep existing native/AUR layout tests unchanged and running in the same gate.
5. Extend QML smoke verification to run against the user-local launcher and
   patched action defaults.
6. Document user-local install first for Atomic/immutable systems, including a
   build-in-container example using `PIROSTATS_BINARY`.
7. Correct README's stale Python runtime/install description while touching the
   install section.

Gate:

```bash
bash -n install.sh uninstall.sh tools/p6_package_test.sh
tools/p6_package_test.sh
tools/qml_verify.sh --smoke
cargo test --manifest-path rust/Cargo.toml --all-targets
```

Run the focused user-install test too if split from `p6_package_test.sh`.

## Test matrix

| Case | Expected result |
|---|---|
| Fresh `--user` install | all files user-owned; applet visible; service active |
| Repeat `--user` install | upgrade succeeds; stale shipped file removed |
| Invalid prebuilt binary | fail before replacing prior install |
| Cargo build failure | fail before replacing prior install |
| Custom absolute `XDG_DATA_HOME` | all data/unit assets use custom root |
| Relative `XDG_DATA_HOME` | fail before writes |
| Missing `kpackagetool6` | fail before replacing prior install |
| Missing optional KDE cache/restart tools | install succeeds with documented fallback |
| Service activation failure | non-zero exit plus recovery/journal commands |
| User uninstall | owned files removed; config retained |
| Repeat user uninstall | no error, no unrelated deletion |
| System install regression | existing `/usr` manifest unchanged |
| `DESTDIR` package staging | existing staged manifest unchanged |
| System and user coexistence | one selected service/applet; no duplicate daemon |
| Checkout removed after install | CLI, daemon, styles, labels still work |
| Path containing spaces | launcher, service, and applet actions still execute |

## Safety requirements

- Resolve and validate every removal root before `rm -rf`; reject empty, `/`,
  `$HOME`, `$XDG_DATA_HOME`, and paths outside the expected PiroStats leaf.
- Quote every path and generated shell argument.
- Never evaluate user-provided strings with `eval`.
- Never source generated launchers or units during install.
- Never use `sudo` in user mode, even when available.
- Build before replacing the current installation.
- Keep config and cache outside the shipped-file replacement transaction.
- Print mode and destination before the first write so mistakes are obvious.

## Documentation outcome

README installation section should present two supported paths:

```bash
# Immutable/Atomic or no root access
./install.sh --user

# Traditional system-wide installation
./install.sh
```

It must explain:

- Rust 1.85+/Cargo build requirement;
- `PIROSTATS_BINARY` for binaries built in Distrobox/Toolbox;
- `$HOME/.local/bin` may need adding to interactive-shell `PATH`, while the
  service/applet use explicit paths and do not depend on it;
- exact uninstall command for each mode;
- config survival and location;
- system/user coexistence and migration limitation discovered during Plasma
  validation.

## Definition of done

- `./install.sh --user` performs no privileged or `/usr` write.
- Installed runtime is independent of checkout and container.
- CLI, daemon service, applet rendering, click, wheel paging, pinning, and hot
  reload work on Plasma 6.
- Upgrade is repeatable and does not leave stale shipped files.
- Uninstall removes only owned user-local files and preserves config.
- Native system install, `DESTDIR`, and AUR package tests do not regress.
- Disposable automated tests cover manifest, commands, upgrade, failure safety,
  and uninstall.
- Live validation passes on Bazzite or another Fedora Atomic KDE image.

## Rollback

Before release, rollback is deletion of the `--user` branches, user unit, and
tests; native packaging remains untouched. After a user has installed it:

```bash
./uninstall.sh --user
```

If service activation failed but files were installed:

```bash
systemctl --user disable --now pirostats
./uninstall.sh --user
```

User configuration remains available for a later fixed release.
