# 02: NPU usage chart and live acceptance

**What to build:** The graphs page gains an `npu` chart of NPU busy history, configurable from the Graphs settings page and present by default after the GPU chart, and the whole feature is accepted live on this machine under real NPU load, as described in the [spec](../spec.md).

**Blocked by:** 01 (AMD NPU tooltip and panel items)

**Status:** ready-for-agent

- [ ] Chart id `npu` joins the known chart list, the default `graph_order` after `gpu`, the shipped config comment, the Graphs settings page (label "NPU usage") and its help text, and settings save validation; it renders NPU busy history on a 0–100 scale, labelled like the other percentage charts, only when the NPU source is detected.
- [ ] The chart is configured only when `pages.order` contains `graphs` and `pages.graph_order` contains `npu`, unlike the existing GPU and network history jobs. While configured, NPU discovery, sampling, and history run even when the page is not presented; removing either condition stops and clears history while panel or presented-tooltip demand continues.
- [ ] History follows `display.history_interval` and `graph_history_length`, resets on source change, and stays separate from the selected-GPU history so the NVIDIA, AMDGPU, then Intel chart selection is unchanged.
- [ ] Tests cover chart presence and absence, chart-only demand, chart removal, removal of all NPU demand, history reset on source change, and the graphs-page render or golden output.
- [ ] Graphs settings smoke check from the development doc passes; Qt render review of the chart; full Rust and repository gates pass.
- [ ] Live acceptance on this machine with the installed build: close any app that holds the NPU open (for example GNOME Resources, which keeps it `active`), then run `tools/npu_fabric_clock.sh with -- tools/npu_load.sh 60` (repo-local FastFlowLM, `qwen3:0.6b`) and confirm tooltip rows and the chart track the spec's measured values (busy about 93–95%, power about 3.1–3.4 W, clock 1810 MHz), return to idle after the load, and that the NPU's `runtime_status` returns to `suspended` while the daemon keeps polling. Confirm FCLK is released afterwards with `tools/npu_fabric_clock.sh status`.
