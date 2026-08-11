# Run laptop low-power validation

Type: task
Status: ready-for-human
Blocked by: 08

## Objective

Validate latency and energy/resource behavior on the real low-power target before shipping.

## Scope

- Compare fixed synchronous and async release commits with identical config, kernel, power mode, display state, services, and scenario duration.
- Measure hidden tooltip, main tooltip, each expensive page, unavailable service, timeout, suspend/resume, and long pinned-tooltip behavior.
- Record CPU time, wakeups/context switches, child-process count, RSS, scheduler latency, publication latency, and available RAPL or battery-energy evidence.
- Repeat enough runs to separate signal from thermal/background noise and attach raw commands/results to this feature directory.

## Acceptance

- Async meets provisional deadline SLOs.
- Hidden idle has no meaningful energy regression.
- Any winning/losing boundary is identified well enough to drive fixed-budget retuning.

## Validation

Human reviews the laptop environment and measurements; automated agents may analyze captured results but must not fabricate unavailable power evidence.
