# 02: NPU usage chart and live acceptance

**What to build:** The graphs page gains an `npu` chart of NPU busy history, configurable from the Graphs settings page and present by default after the GPU chart, and the whole feature is accepted live on this machine under real NPU load, as described in the [spec](../spec.md).

**Blocked by:** 01 (AMD NPU tooltip and panel items)

**Status:** ready-for-agent

- [x] Chart id `npu` joins the known chart list, the default `graph_order` after `gpu`, the shipped config comment, the Graphs settings page (label "NPU usage") and its help text, and settings save validation; it renders NPU busy history on a 0–100 scale, labelled like the other percentage charts, only when the NPU source is detected.
- [x] The chart is configured only when `pages.order` contains `graphs` and `pages.graph_order` contains `npu`, unlike the existing GPU and network history jobs. While configured, NPU discovery, sampling, and history run even when the page is not presented; removing either condition stops and clears history while panel or presented-tooltip demand continues.
- [x] History follows `display.history_interval` and `graph_history_length`, resets on source change, and stays separate from the selected-GPU history so the NVIDIA, AMDGPU, then Intel chart selection is unchanged.
- [x] Tests cover chart presence and absence, chart-only demand, chart removal, removal of all NPU demand, history reset on source change, and the graphs-page render or golden output.
- [x] Graphs settings smoke check from the development doc passes; Qt render review of the chart; full Rust and repository gates pass.
- [x] Live acceptance on this machine with the installed build: close any app that holds the NPU open (for example GNOME Resources, which keeps it `active`), then run `tools/npu_fabric_clock.sh with -- tools/npu_load.sh 60` (repo-local FastFlowLM, `qwen3:0.6b`) and confirm tooltip rows and the chart track the spec's measured values (busy about 93–95%, power about 3.1–3.4 W, clock 1810 MHz), return to idle after the load, and that the NPU's `runtime_status` returns to `suspended` while the daemon keeps polling. Confirm FCLK is released afterwards with `tools/npu_fabric_clock.sh status`.

## Comments

Live acceptance on 2026-10-17 with the installed build, FCLK held, `tools/npu_load.sh 60` (12 requests): tooltip rows settled at 88–94% busy, 2.9–3.3 W, 1810 MHz; the chart legend tracked the same values and the chart drew the ramp, plateau, and fall. Busy fell to 0% within about 4 s of the last request, the NPU returned to `suspended` about 5 s after the load, and stayed suspended for the next 50 s while the daemon kept sampling for the chart (rows `0%`, `0.0 W`, `off`). `tools/npu_fabric_clock.sh status` afterwards: released (auto). The Graphs settings smoke check's expected order was stale (it predated the `disk` chart) and was updated with the `npu` entry.
