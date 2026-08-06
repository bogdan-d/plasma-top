# Introduce domain model and owner state

Type: task
Status: ready-for-agent
Blocked by: 01

## Objective

Make independent sampling expressible without changing production scheduling yet.

## Scope

- Rename `ReadingsSnapshot` to `DisplaySnapshot`, `collected_at` to `assembled_at`, and `HardwareSnapshot` to `HardwareInventory`.
- Introduce `MetricSample<T>` and typed per-domain owner result/state structures.
- Split `CollectorState` and `DaemonStateSnapshot` caches into their owning domains; retain notification state separately.
- Extract one-attempt domain reads from hidden TTL policy while a temporary synchronous coordinator preserves current order and intervals.
- Add validated cadence types and reject non-finite or sub-100 ms configured durations with last-good reload behavior.

## Acceptance

- Rendered fixtures and notification behavior remain unchanged for valid config.
- Domain state has one obvious owner and no cross-domain mutable cache bundle remains.
- Existing sensor fixture tests remain synchronous and focused.

## Validation

Run the full Rust gates from `docs/DEVELOPMENT.md`.
