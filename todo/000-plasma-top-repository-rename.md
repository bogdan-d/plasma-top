# PlasmaTop repository-wide identity rename

## Status

Planned. The repository is already named `plasma-top`; repository hosting,
checkout-directory, and Git-remote renames are outside this plan.

This file is intentionally temporary because it names the legacy identity to
describe its removal. Delete this file after the rename. Final verification
must run after deletion so the completed repository contains no `pirostats`,
`PiroStats`, `PIROSTATS`, `plasma-stats`, or derivative references.

## Goal

Replace every repository identity derived from `pirostats` with one coherent
PlasmaTop identity. This includes tracked content, tracked paths, generated
artifacts, executable and crate names, Plasma package metadata, environment
variables, installed paths, state namespaces, packaging, tests, documentation,
and developer tooling.

This is a clean break. Do not retain compatibility aliases, deprecated
environment variables, old package conflicts, migration readers, forwarding
executables, or cleanup code containing the legacy identity.

## Canonical identity

Use these values everywhere:

| Surface | Value |
|---|---|
| Repository | `plasma-top` |
| Website | `https://github.com/bogdan-d/plasma-top` |
| Display name | `PlasmaTop` |
| Executable and filesystem slug | `plasma-top` |
| Rust package and binary | `plasma-top` |
| Rust crate identifier | `plasma_top` |
| Environment prefix | `PLASMA_TOP_` |
| Plasma applet ID | `com.github.bogdan-d.plasma-top` |
| Icon name | `plasma-top` |
| systemd unit | `plasma-top.service` |
| AUR package | `plasma-top-git` |

The Plasma applet ID is inferred once from the canonical repository URL:
reverse `github.com`, append owner `bogdan-d`, append repository `plasma-top`.
It is then a static installed identity. Do not derive it dynamically from the
checkout basename: installed packages have no Git checkout, and changing an
installed applet ID loses Plasma's widget identity.

## Current scope

Initial discovery found 579 exact legacy-name occurrences across 81 tracked
files and seven legacy-named tracked paths. Ignored Rust build output also
contains legacy crate and binary names. The main coupled contracts are:

- Cargo package, binary, lockfile package, and Rust crate imports;
- daemon/QML agreement on the XDG runtime directory;
- Plasma metadata ID, installer destination, geometry lookup, and test launch
  ID;
- systemd unit, launcher, packaged binary, and installed asset root;
- command defaults embedded in KConfig XML;
- XDG config/cache paths and install ownership markers;
- AUR source/package/install-hook names;
- CLI help, notifications, tests, docs, CI, tools, and local skill guidance.

## Implementation plan

### 1. Rename tracked paths

Use `git mv` so history remains legible:

```text
pirostats
  -> plasma-top
packaging/pirostats-launcher
  -> packaging/plasma-top-launcher
packaging/aur/pirostats.install
  -> packaging/aur/plasma-top.install
service/pirostats.service
  -> service/plasma-top.service
service/pirostats-user.service
  -> service/plasma-top-user.service
plasmoid/package/contents/icons/pirostats.svg
  -> plasmoid/package/contents/icons/plasma-top.svg
.agents/skills/plasma-qml/rules/pirostats-contracts.md
  -> .agents/skills/plasma-qml/rules/plasma-top-contracts.md
```

Update every caller immediately after each rename. Do not leave duplicate old
and new files, symlinks, or wrapper launchers.

### 2. Rename Rust package and process identity

Update `rust/Cargo.toml`:

- package name to `plasma-top`;
- binary name to `plasma-top`;
- comments and descriptions to PlasmaTop.

Regenerate `rust/Cargo.lock` through Cargo; do not hand-edit unrelated lockfile
entries. Change Rust imports from `pirostats::` to `plasma_top::`, including
`rust/src/main.rs` and integration tests.

Rename all process-facing text:

- CLI usage, help, version, and parser errors in `rust/src/cli.rs`;
- expected CLI text in `rust/tests/cli_daemon.rs`;
- process/module documentation in `rust/src/{main,lib}.rs`;
- notification title in `rust/src/notify.rs` and notification fakes;
- dependency rationale and comments in `rust/Cargo.toml` and
  `rust/DEPENDENCIES.md`.

Rename Rust identifiers derived from the old product name, such as
`PIROSTATS_CODE_ROOT_ENV` and `is_pirostats`. Use `PLASMA_TOP_CODE_ROOT_ENV`
and terminology based on applet/plugin identity instead.

### 3. Rename config, runtime, cache, and diagnostic namespaces

Update asset resolution in `rust/src/config/assets.rs`:

- `PIROSTATS_CODE_ROOT` to `PLASMA_TOP_CODE_ROOT`;
- `/usr/lib/pirostats` to `/usr/lib/plasma-top`;
- `$XDG_CONFIG_HOME/pirostats` and `~/.config/pirostats` to `plasma-top`;
- associated tests, examples, and diagnostics.

Update runtime agreement atomically in:

- `rust/src/runtime/mod.rs`;
- `plasmoid/package/contents/ui/main.qml`;
- runtime path tests under `rust/src/runtime/` and `rust/tests/`;
- runtime examples in config and docs.

New paths are:

```text
$XDG_RUNTIME_DIR/plasma-top
/tmp/plasma-top-<uid>
<runtime>/panel.html
<runtime>/tooltip.html
<runtime>/state/*
```

Preserve the watched-directory contract: only `panel.html` and `tooltip.html`
persist directly under the runtime root; mutable state remains under `state/`.

Update persistent and diagnostic paths:

- `~/.cache/pirostats/geom` to `~/.cache/plasma-top/geom` in
  `rust/src/config/geometry.rs`;
- `/tmp/pirostats_render_*` to `/tmp/plasma-top_render_*` in
  `rust/src/diagnostics.rs`, `tools/qt_shot.py`, and
  `tools/p6_qt_matrix.sh`;
- all temporary-directory prefixes in Rust unit/integration tests, sensor
  tests, fixture helpers, daemon tests, and shell tests.

Do not add fallback reads from old config/cache/runtime locations.

### 4. Rename the Plasma package contract

Change `plasmoid/package/metadata.json`:

```text
KPlugin.Name:        PlasmaTop
KPlugin.Id:          com.github.bogdan-d.plasma-top
KPlugin.Icon:        plasma-top
KPlugin.Website:     https://github.com/bogdan-d/plasma-top
```

Rewrite its description to name the PlasmaTop daemon. Preserve authors,
license, category, version, package structure, and Plasma API version unless a
separate change requires otherwise.

Change every applet-ID consumer in the same commit:

- `rust/src/config/geometry.rs` plugin lookup and tests;
- `install.sh` and `uninstall.sh` package paths;
- `packaging/aur/PKGBUILD` package destination;
- `tools/p6_package_test.sh` layout assertions;
- `tools/qml_verify.sh` and `tools/p6_live_matrix.sh` launch/staging logic.

Change icon consumers to `plasma-top.svg`. Update command defaults in
`plasmoid/package/contents/config/main.xml` from `/usr/bin/pirostats` to
`/usr/bin/plasma-top`. Update related QML/config descriptions and tests.

The following values must always match exactly:

1. `KPlugin.Id` in metadata;
2. Plasma package install directory;
3. Rust appletsrc plugin lookup;
4. installer/uninstaller applet ID;
5. QML/package test applet ID.

Keep the existing repository gate or package tests responsible for detecting
drift; do not introduce runtime JSON parsing or a build script solely to share
this literal.

### 5. Rename installation and service surfaces

Update the checkout launcher, packaged launcher, `install.sh`, and
`uninstall.sh` to use:

```text
./plasma-top
/usr/bin/plasma-top
/usr/lib/plasma-top/plasma-top
~/.local/bin/plasma-top
$XDG_DATA_HOME/plasma-top/plasma-top
PLASMA_TOP_CODE_ROOT
PLASMA_TOP_BINARY
```

Rename all install implementation details:

- ownership marker to `.plasma-top-install`;
- stage/backup/applet temporary prefixes;
- user and system license directories;
- icon destinations;
- dry-run output and recovery instructions;
- command replacement performed in staged KConfig XML;
- safety checks and package ownership assertions.

Update both systemd unit files:

- filename and unit name to `plasma-top.service`;
- display description to PlasmaTop;
- `ExecStart` to the matching system or user-local launcher;
- documentation URL to the canonical repository URL.

Update all `systemctl` and `journalctl` calls in installers, uninstallers,
package hooks, tests, and documentation. Preserve existing activation,
restart, ownership, safe-path, dry-run, and user-file-preservation behavior.

### 6. Rename Arch/AUR packaging

Update `packaging/aur/PKGBUILD`:

- `pkgname=plasma-top-git`;
- `_reponame=plasma-top`;
- canonical URL and Git source;
- `provides=('plasma-top')`;
- `conflicts=('plasma-top')`;
- release binary, launcher, service, icon, applet, library, and license paths;
- install hook name to `plasma-top.install`.

Update `packaging/aur/plasma-top.install` messages and commands. Do not retain
old `provides` or `conflicts` entries: those are legacy references and would
also imply an unsupported in-place migration.

### 7. Rename tests, tools, CI, documentation, and local guidance

Update rename-sensitive checks first so failures expose missed production
surfaces:

- `tools/repository_gate.sh`;
- `tools/user_install_test.sh`;
- `tools/p6_package_test.sh`;
- `tools/qml_verify.sh`;
- `tools/p6_live_matrix.sh`;
- `tools/p6_qt_matrix.sh`;
- `tools/qt_shot.py`;
- `.github/workflows/baseline.yml`.

Then update remaining content in:

- `README.md`, `NOTICE`, and `AGENTS.md`;
- `docs/**` and `todo/**`;
- `config/**`, `style/**`, and QML configuration text;
- `.agents/skills/plasma-qml/**`;
- all Rust source, tests, comments, test names, fixture prefixes, and expected
  strings.

Update `docs/DEVELOPMENT.md` last so its commands describe the final filenames,
environment variables, launchers, and validation gates accurately.

Add a permanent repository-gate check that fails when tracked content or paths
contain any forbidden legacy identity variant. Keep the check narrow enough
not to flag unrelated words, but cover case, hyphen, underscore, and compact
forms.

### 8. Remove generated legacy artifacts

Ignored `rust/target/` output contains old crate fingerprints, binaries, and
metadata. After explicit approval for generated-file deletion, run:

```bash
cargo clean --manifest-path rust/Cargo.toml
```

Remove ignored test/render artifacts containing the old identity. Rebuild from
the renamed checkout so compile-time `CARGO_MANIFEST_DIR` paths also contain
the canonical repository name.

Do not rewrite Git history as part of this task. Current-tree identity and Git
history are separate scopes; history rewriting is destructive and requires an
explicit, separate decision.

## Migration policy

Changing the applet ID, service, executable, and XDG namespaces means existing
installations do not migrate automatically:

- existing Plasma widget instances retain the old plugin ID;
- old config, cache, and runtime state are not read;
- old service state is not disabled by the new installer;
- both daemons could coexist until the old installation is removed.

Any in-repository migration would have to preserve forbidden strings and would
contradict the zero-reference requirement. Publish one-time cleanup guidance in
external release notes if needed. Keep no migration code or compatibility note
in the completed repository.

## Validation

### Identity checks

Before deleting this plan, use searches to find remaining work. Delete this
file, then rerun all checks against the final tree.

Tracked content and paths must produce no output:

```bash
git grep -IniE 'piro|plasma[-_ ]?stats'
git ls-files | grep -Ei 'piro|plasma[-_ ]?stats'
```

The working tree outside `.git` must contain no legacy-named paths or ignored
generated content:

```bash
find . -path ./.git -prune -o -iname '*piro*' -print
rg -i --hidden --glob '!.git/**' 'piro|plasma[-_ ]?stats' .
```

Confirm canonical values and coupling:

- Cargo builds `plasma-top` and Rust imports use `plasma_top`;
- CLI help, errors, and notifications use PlasmaTop/plasma-top;
- daemon and QML resolve the same `plasma-top` runtime root;
- metadata, geometry lookup, installers, package paths, and tests use
  `com.github.bogdan-d.plasma-top`;
- systemd units execute the correct launchers;
- config/cache/runtime/install paths use only `plasma-top`;
- AUR package builds and installs only the canonical identity.

### Required repository gates

Run the final commands documented in the renamed `docs/DEVELOPMENT.md`,
including:

```bash
cargo fetch --locked --manifest-path rust/Cargo.toml
git diff --exit-code -- rust/Cargo.lock
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
tools/repository_gate.sh
bash -n install.sh uninstall.sh packaging/aur/PKGBUILD \
  packaging/aur/plasma-top.install plasma-top tools/*.sh scripts/*.sh
tools/user_install_test.sh
tools/p6_package_test.sh
```

For Plasma/QML integration:

```bash
tools/p6_qt_matrix.sh --no-build
tools/qml_verify.sh --smoke
```

Run the smoke check on a supported Plasma host. Manually verify widget
discovery, daemon activation, panel orientation detection, runtime publication,
tooltip paging, wheel/click commands, pinning, and uninstall behavior.

## Done when

- Every canonical identity value is applied consistently.
- All seven tracked paths are renamed with no compatibility copies.
- No tracked content, tracked path, ignored artifact, or checkout-relative
  generated output contains a legacy identity or derivative.
- Plasma metadata, geometry lookup, installation paths, and launch tools agree
  on one applet ID.
- Full Rust, repository, packaging, Qt, and Plasma checks pass.
- Existing runtime/layout contracts remain unchanged apart from namespace.
- This plan is deleted and the zero-reference searches still pass.
