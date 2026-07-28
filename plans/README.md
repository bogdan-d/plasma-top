# PiroStats Rust migration program (historical)

P8.5 and the final migration gate completed on 2026-07-29. This directory is
retained as rollback, parity, and decision evidence; it no longer directs
normal development. Current architecture and gates live in `AGENTS.md`,
`docs/`, and `.github/workflows/baseline.yml`.

This directory was the execution source of truth for replacing the Python
backend with Rust while preserving the Plasma applet's observable behavior. The
work was a compatibility rewrite, not a redesign.

## Goal

Ship one Rust `pirostats` binary that replaces `pirostats` and `src/*.py`, while
keeping these assets and contracts compatible:

- Plasma applet (`plasmoid/`), including its file watch and geometry publication
- `config/*.toml`, `style/*`, and `lang/*`
- systemd user service and CLI command names
- runtime directory layout and atomic publication behavior
- rendered panel/tooltip appearance and interaction
- supported sensors, graceful degradation, caching, and notifications

Python remained the behavioral oracle until final cutover. No Python/Rust FFI or
production split-brain daemon was introduced.

## Read order

1. [CONTRACT.md](CONTRACT.md) — behavior that must not drift
2. [ARCHITECTURE.md](ARCHITECTURE.md) — target Rust ownership and boundaries
3. [PHASES.md](PHASES.md) — ordered program, gates, rollback points
4. [LANES.md](LANES.md) — parallel work ownership and dependency waves
5. [TESTING.md](TESTING.md) — exactness policy and validation matrix
6. [INVENTORY.md](INVENTORY.md) — every tracked file and Python callable
7. [HANDOFF.md](HANDOFF.md) — agent branch/worktree and handoff protocol
8. [STATUS.md](STATUS.md) — integration-owner progress ledger

Deferred issues found during migration live in
[`POST_MIGRATION_ISSUES.md`](POST_MIGRATION_ISSUES.md) for investigation after
P8.5.

Agent completion reports go under [`handoffs/`](handoffs/README.md), one file per
lane/attempt. Agents do not edit `STATUS.md`; the integration owner updates it
after verifying a handoff.

## Program invariants

1. Preserve behavior before improving it.
2. Keep QML/CSS/TOML contracts stable during backend replacement.
3. Every current callable receives an explicit disposition and test evidence in
   `INVENTORY.md`; no function disappears accidentally.
4. Every external call gets success, absence, malformed-result, timeout/failure,
   and permission/error coverage where applicable.
5. Pure deterministic output is byte-compared with the Python oracle.
6. Hardware I/O uses captured fixtures first, then live-machine validation.
7. One lane owns each implementation path. Shared API changes go through the
   integration owner; agents do not concurrently edit shared modules.
8. No cutover until all mandatory gates pass with no unexplained skips.
9. Do not weaken tests, lints, error handling, or parity rules to finish a lane.
10. Keep rollback possible at every phase boundary.

## Execution graph

```text
P0 baseline/oracle
  -> P1 Rust scaffold + frozen contracts
      -> [P2 domain/registry | P2 config | P2 runtime/page | P2 fixture framework]
          -> [P3 render core/traces | P3 CPU/mem/net/disk sensors]
              -> [P4 formatter/pages/chart | P4 process/D-Bus/GPU/HID | P4 notifier]
                  -> P5 daemon + CLI integration
                      -> P6 QML/runtime + packaging validation
                          -> P7 shadow runs, hardware matrix, cutover
                              -> P8 Python removal and stabilization
```

Brackets are parallel waves. Details and file ownership live in `LANES.md`.

## Definition of exact parity

“Exact” means observable equivalence, not necessarily identical implementation:

- byte-identical deterministic text/HTML/config/CLI output
- identical chart pixels and dimensions; compressed PNG bytes also match when
  the encoder permits deterministic parity
- identical state transitions, cache cadence, page wrapping, and atomic writes
- sensor values derived with the same formulas; volatile readings compared with
  defined tolerances from simultaneous samples
- Qt-rendered screenshots match within the environment-specific pixel tolerance
  documented in `TESTING.md`
- same graceful behavior when hardware, commands, libraries, files, or D-Bus
  services are absent

Any accepted deviation requires an ADR-style entry in `STATUS.md`, user approval,
and updated oracle evidence. Silent “close enough” changes are forbidden.
