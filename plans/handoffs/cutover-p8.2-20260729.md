# CUTOVER P8.2 handoff — Codex — 2026-07-29

## Result

P8.2 live stabilization was explicitly accepted by the user on 2026-07-29. One
installed user-local Rust session covered clean service stop and
restart, healthy publication, panel and tooltip refresh, click, wheel paging in
both directions, middle-click pin/unpin, config and style hot reload, rollback
of test overrides, and a final clean stop/restart. No Rust backend defect was
found, so no production code was changed. P8.4 has not started.

## Baseline and installed mode

- Window: 2026-07-29 00:24–00:32 EEST.
- Branch: `rust-migration-base-bootstrap`.
- Start: clean tree at `fc65079`, a descendant of requested base `9dc9efb`.
- Host: Bazzite 44.20260721.0 (Kinoite), kernel
  `7.1.3-ogc5.1.fc44.x86_64`, Plasma 6.7.3, KDE Wayland session.
- Mode: supported user-local installation.
- Unit: `~/.local/share/systemd/user/pirostats.service`.
- Launcher: `~/.local/bin/pirostats`, exporting
  `PIROSTATS_CODE_ROOT=/var/home/bogdan/.local/share/pirostats`.
- Process executable: `~/.local/share/pirostats/pirostats`, an ELF Rust binary;
  SHA-256 `542f54c4cdbf69f6054abc0bd246363a600877c04ac29eb4372eaf401a946acc`.
- Runtime: `/run/user/1000/pirostats`; only `panel.html` and `tooltip.html`
  persisted at its root, with page/geometry state under `state/`.

## Live session evidence

### Lifecycle and publication

`systemctl --user stop pirostats.service` reached `inactive`, followed by a
successful start with a new PID. PID changed from `1839106` to `1849439`.
Within three seconds both runtime HTML files were nonempty and current. The
final cleanup repeated a clean stop/start and left PID `1858709` active/running
with `ExecMainStatus=0` and fresh panel/tooltip files.

Relevant journal excerpt:

```text
00:25:30 systemd: Stopping pirostats.service...
00:25:30 systemd: Stopped pirostats.service.
00:25:30 systemd: Started pirostats.service.
00:25:30 pirostats: [boot] first paint at +19ms
00:25:30 pirostats: [boot] hd_temps ready at +0.02s
00:31:31 systemd: Stopping pirostats.service...
00:31:31 systemd: Stopped pirostats.service.
00:31:31 systemd: Started pirostats.service.
00:31:31 pirostats: [boot] first paint at +18ms
00:31:31 pirostats: [boot] hd_temps ready at +0.02s
```

No warning, error, failure, or panic appeared in the daemon unit journal.

### Human-only Plasma checks

The user performed these interactions against the live installed widget and
reported each as passing; no automated substitute is claimed as visual proof:

- panel populated and visibly refreshed after restart;
- hover tooltip opened, populated, and visibly refreshed;
- left click opened Plasma System Monitor;
- one wheel-down gesture moved full stats to `GRAPHS`, then one wheel-up gesture
  returned to full stats;
- middle click pinned the tooltip across pointer movement/focus loss, then a
  second middle click unpinned it.

Runtime corroboration after interaction showed `npages=3`, final `page=0`, a
page-state mtime during the interaction, and continuously updated panel and
tooltip mtimes. Human confirmation remains the authoritative evidence for the
rendered and input behavior.

### Config and style hot reload

No user override existed before the test. Installed defaults were copied into a
temporary `~/.config/pirostats/` tree, byte-checked against installed assets,
then the service was restarted once so those paths were the live watch targets.

1. Config-only edit changed the first tooltip title from `CPU & MEM` to
   `P8.2 CONFIG RELOAD`. Runtime HTML reflected it in about 1.0 seconds without
   restart; the user confirmed the visible live change and healthy tooltip.
2. Style-only edits changed the active tooltip-title color to `#ff00ff`.
   Runtime HTML reflected it in under 0.3 seconds without restart; the user
   confirmed the visible magenta title and continuing refresh.
3. Copying original config and styles back hot-restored `CPU & MEM` and the
   normal theme color without restart; the user confirmed both.
4. Test-created files and directories were removed, then a final clean
   stop/start restored the shipped-default path. `~/.config/pirostats` is absent
   again and runtime HTML contains neither test marker.

## Defects and fixes

- Rust/backend defects: none observed. No Rust or QML workaround was made.
- Plasma journal observation: at 00:26:46, `ConfigAppearance.qml` emitted
  `Setting initial properties failed` messages for generated `cfg_*`
  properties. Core P8.2 interactions all passed, the daemon journal stayed
  clean, and this did not indicate a backend failure. Recorded as a non-blocking
  QML/config-loader diagnostic under `PM001` in
  `plans/POST_MIGRATION_ISSUES.md`; not patched during backend stabilization.

## Exceptions and rollback

- D005 remains accepted: no pacman/system-wide lifecycle was induced.
- D006 remains accepted: no unavailable Intel/NVIDIA/battery/HID path, suspend,
  route switch, or hotplug was induced.
- Annotated tag `pre-rust-cutover` remains present and resolves to `31ec788`.
- Python oracle source/tests remain in-tree and uninstalled. Rollback remains
  available through the tag and previously verified package/user-local
  uninstall paths.
- No rollback was needed. All temporary user configuration was removed and the
  installed Rust service remains healthy.

## Full gates

All required gates passed after the live window:

```text
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
.venv/bin/python -m pytest tests/ -v
.venv/bin/ruff check .
.venv/bin/vulture src/ tests/ tools/python_oracle.py tests/vulture_whitelist.py --min-confidence 60
```

Rust: 507 library + 26 integration tests passed. Python: 175 passed, one
optional skip. Ruff and Vulture passed.

## Acceptance

The user explicitly accepted P8.2 on 2026-07-29 after reviewing the verified
live checklist and gate result. P8.2 is closed. P8.4 Python removal was not
started as part of this handoff.
