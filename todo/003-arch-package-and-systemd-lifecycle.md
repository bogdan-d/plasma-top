# Real Arch package and user-systemd lifecycle

## Status

Deferred. Disposable staging tests pass; real package-manager and user-service
lifecycle remains unverified.

## Residual risk

`tools/p6_package_test.sh` verifies manifests, legacy-layout upgrades, repeat
upgrades, uninstall behavior, AUR packaging, and user-file preservation under a
temporary root. It cannot prove real pacman hooks or user-systemd enablement on
an Arch Plasma installation.

## Relevant files

- `packaging/aur/PKGBUILD`
- `packaging/aur/pirostats.install`
- `packaging/pirostats-launcher`
- `service/pirostats.service`
- `service/pirostats-user.service`
- `install.sh`
- `uninstall.sh`
- `tools/p6_package_test.sh`

## Handoff

Use a disposable Arch Plasma machine or snapshot-capable VM.

1. Build the package with the supported AUR flow.
2. Test clean install, service enable/start, applet discovery, runtime
   publication, stop/start, and login restart.
3. Test upgrade from a prior native package and a legacy Python-layout package
   if such an artifact remains available.
4. Test repeat upgrade and uninstall.
5. Verify user config/cache preservation and removal of package-owned files.
6. Capture pacman and user-journal evidence; restore the machine snapshot.

## Done when

- Real pacman install/upgrade/uninstall succeeds.
- User-systemd lifecycle and applet operation succeed after login restart.
- Package-owned versus user-owned file behavior matches documented contracts.
- `tools/p6_package_test.sh` and all gates in `docs/DEVELOPMENT.md` remain green.

