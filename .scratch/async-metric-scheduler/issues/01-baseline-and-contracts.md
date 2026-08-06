# Capture baseline and scheduler contracts

Type: task
Status: ready-for-agent

## Objective

Freeze reproducible synchronous behavior and performance evidence before structural changes.

## Scope

- Record release profiling for hidden-equivalent, main tooltip, configured pages, unavailable command/service, and timeout fixtures on the development host.
- Add focused characterization tests only where scheduler extraction would otherwise rely on implicit ordering, retry, history, first-paint, reload, notification, or page behavior.
- Record the exact baseline commit and commands in this feature directory; do not treat development-host timings as laptop power evidence.

## Acceptance

- Baseline commit, host/config, commands, and results are reproducible.
- Every load-bearing behavior needed by later tickets has either an existing cited test or one focused characterization test.
- No production behavior changes.

## Validation

Run the full Rust gates from `docs/DEVELOPMENT.md`.
