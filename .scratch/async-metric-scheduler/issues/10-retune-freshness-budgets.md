# Retune freshness budgets

Type: task
Status: needs-info
Blocked by: 09

## Objective

Use laptop A/B evidence to tune fixed per-job freshness budgets without adding a generic adaptive controller.

## Scope

- Change only budgets with measured energy/latency benefit and acceptable metric age.
- Preserve demand scoping, bounded failure backoff, histories, notification requirements, and configured SMART behavior.
- Consider native netlink route detection only if command process evidence makes it material.
- Update `docs/PERFORMANCE.md` with current Rust measurements, host/scenarios, and final policy.

## Acceptance

- Every changed budget cites laptop evidence and user-visible freshness impact.
- Final A/B still meets deadline SLOs and improves or preserves hidden-idle energy.
- No user-facing concurrency knobs or unmeasured adaptive policy are added.

## Validation

Repeat affected laptop scenarios and run the full repository gates from `docs/DEVELOPMENT.md`.
