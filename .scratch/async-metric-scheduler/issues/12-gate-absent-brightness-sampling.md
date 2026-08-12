# Gate absent brightness sampling

Type: task
Status: resolved
Blocked by: 08

## Objective

Stop display-rate brightness attempts after hardware inventory confirms that no readable backlight exists, while preserving periodic discovery and correct hotplug activation.

## Evidence

The issue-08 development host has `HardwareInventory.has_backlight = false`, and rendering correctly hides `screen_brightness`. Every 4.2-second steady main, graphs, and processes run nevertheless dispatched six `External:Brightness` attempts, all of which failed. Failure backoff is capped by the display freshness budget, so a presented tooltip can continue probing absent sysfs backlight data every display interval.

Raw reports are `.scratch/async-metric-scheduler/runs/development/candidate-final/{normal,affinity-cpu0}-{main,graphs,processes}-*.out`; methodology and host limitations are in `.scratch/async-metric-scheduler/development-validation.md`.

## Scope

- Make brightness demand depend on confirmed backlight inventory rather than configuration alone.
- Keep the 60-second backlight inventory job demanded whenever a configured surface needs brightness so newly available hardware can still be discovered.
- On backlight appearance, add and immediately schedule the brightness job through normal inventory reconciliation.
- On confirmed removal, cancel the brightness job and invalidate its retained sample and render input.
- Preserve failure/backoff behavior for a discovered backlight whose files become transiently unreadable; absence and boundary failure must remain distinct.
- Keep brightness labels, rendering, configuration, and other external-file jobs unchanged.

## Acceptance

- A configured brightness item with `has_backlight = false` dispatches inventory reconciliation but no `Brightness` sampling job.
- Confirmed backlight appearance schedules sampling, confirmed removal invalidates the sample, and transient read failure retains the last successful value with bounded backoff.
- A repeated 4.2-second presented development-host profile reports zero brightness sampling attempts when no backlight exists.
- First-paint, page/control, publication, and shutdown SLOs remain satisfied.

## Validation

Run focused catalog, inventory-reconciliation, invalidation, external-sensor, render, and async-loop tests; repeat steady `main` profiling without a backlight; then run the full repository gates from `docs/DEVELOPMENT.md`.

## Answer

Brightness sampling jobs now require confirmed backlight inventory, while configured brightness surfaces retain the independent 60-second Backlight inventory demand. Inventory appearance adds and immediately schedules Brightness through normal reconciliation; removal cancels the job and invalidates only brightness data. A discovered device's transient read failure retains its last-good value and follows scheduler backoff, and unrelated external-file jobs are unchanged.

On the no-backlight development host, all three 4.2-second release `main` runs changed from six failed brightness attempts to zero while retaining one successful Backlight inventory attempt. Worst first paint was 105.254 ms, publication p99 1.822 ms, shutdown 6.703 ms, and no display deadline or SLO threshold was missed. Focused tests and all repository gates passed. Commands, hashes, raw reports, and limitations are in `.scratch/async-metric-scheduler/runs/issue-12/`.
