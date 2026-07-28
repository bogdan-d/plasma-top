# Agent handoff runbook

## Roles

- **Integration owner:** assigns lanes, freezes shared APIs, verifies/merges,
  updates `STATUS.md`, runs aggregate gates.
- **Lane agent:** implements only assigned objective/paths and writes evidence.
- **Hardware agent:** runs approved read-only/live validation; does not edit code.

## Starting a lane

1. Read `README.md`, `CONTRACT.md`, relevant phase, lane contract, testing section,
   project `CLAUDE.md`, and applicable source/docs completely.
2. Confirm dependency lanes are marked integrated in `STATUS.md`.
3. Create branch/worktree from listed integration SHA.
4. Copy `handoffs/TEMPLATE.md` to a unique file:
   `handoffs/<lane>-<agent-or-attempt>.md`.
5. Record owned/forbidden paths and baseline checks before editing.
6. If required API is missing or incompatible, stop and request integration-owner
   decision; do not edit another lane's path.

## During implementation

- Keep commits small and lane-owned.
- Preserve current behavior; add fixture/oracle evidence with each behavior.
- Update callable/file inventory entries only in the lane handoff. Integration
  owner applies canonical `INVENTORY.md` updates to avoid conflicts.
- Record every command and unexpected difference immediately.
- Do not regenerate golden output unless Python oracle proves intended behavior
  and integration owner approves.
- No blanket lint allows, skipped tests, broad `Any`/string fallbacks, swallowed
  errors, or unreviewed dependencies.
- Never access production runtime paths from tests.

## Finishing a lane

1. Rebase onto current integration SHA if requested.
2. Run focused tests, full Rust checks appropriate to compiled dependencies, and
   unaffected Python oracle checks.
3. Inspect git diff for out-of-scope files and generated artifacts.
4. Complete handoff template with commits, changed paths, tests/results, parity
   evidence, inventory dispositions, dependencies, risks, and exact blockers.
5. Mark lane **ready for verification**, not complete.
6. Integration owner reruns claims, merges, updates status/inventory, then marks
   integrated.

## Integration order

Within each wave:

1. merge lowest-level contract/pure lane
2. rerun its checks
3. rebase dependent handoffs if API changed
4. merge adapters
5. merge orchestration last
6. run aggregate gate and differential corpus

Integration fixes stay in an integration-owned commit and name affected lanes.
Do not smuggle behavior changes into conflict resolution.

## Conflict policy

- Two lanes touching same implementation file means lane design failed. Pause and
  re-split ownership.
- Shared type addition: integration owner implements or assigns a tiny blocking
  contract lane, then dependent lanes rebase.
- Fixture schema change: BASE/FIXTURES owner versions it; all consumers update in
  one integration wave.
- Golden mismatch: classify Python bug, Rust bug, nondeterminism, environment, or
  accepted change. No normalization before classification.
- Dependency disagreement: choose smallest dependency meeting exact contract;
  defer optional convenience dependencies.

## Status vocabulary

- `blocked` — dependency/decision unavailable
- `ready` — dependencies integrated; unclaimed
- `active` — one assigned owner
- `handoff` — agent reports done, awaiting verification
- `integrated` — integration owner merged and reran lane gate
- `verified` — integrated lane whose cited aggregate evidence was rerun and
  accepted by the integration owner
- `rejected` — handoff failed scope/parity/quality; new attempt required
- `deferred` — explicitly outside current phase

## Review checklist

- Scope and owned paths respected.
- Every referenced current symbol has disposition/test evidence.
- Expected and failure paths asserted.
- No hidden host I/O in deterministic tests.
- Rust errors preserve actionable context.
- No recoverable `unwrap`/panic.
- No unnecessary clones/allocations in poll hot path; no optimization theater.
- No new async/background lifecycle.
- Exact oracle differences are zero or documented/approved.
- Commands independently reproducible.
- Docs/comments describe current invariant, not migration history.
