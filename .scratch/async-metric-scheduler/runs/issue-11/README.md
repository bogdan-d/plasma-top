# Issue 11 graph-render coalescing evidence

Captured on the development host from base `364bba087f73447d31ffa76fdba8d253f7c386be`. The candidate identity is the base plus the source diff recorded in `after/identity.txt`; the release binary SHA-256 is `9fd6168133237c954bc6c5b39afc535c068da77ea37ce058a4d8e64b157d4192`. Commands, raw reports, `/usr/bin/time -v` output, and input hashes are retained in `before/`, `after/`, and `stimuli/`. Timed profiling used disposable state and did not write daemon runtime HTML.

## Diagnosis

Each profiling attempt corresponds to a scheduler dispatch. Accepted and rejected `PageRender` completions executed `render_page_cached` and graph PNG rasterization in the blocking lane; only cancelled attempts can be removed before rasterization. The previous runtime advanced one global render generation after every accepted non-page completion and immediately requested another graph render. Independent startup and graph-source completions therefore invalidated work already rasterizing. Scheduler pending state limited overlap, but quick renders completed between source completions and allowed sequential churn.

The fix advances the generation only for values consumed by `format_graphs`, graph-input invalidation, graph-relevant inventory, width, style, and config changes. Publication requests a graph only when the selected tooltip needs a current frame, and a run-ID reservation permits one in-flight render plus one newest-generation follow-up. A stale retained graph remains visible for activation while a current frame renders; actual page changes still publish the selected-page placeholder. Cancellation clears only its matching reservation, and config page reordering reconciles selected-page demand.

Focused tests classify the disposition paths: graph-input and old-style completions are rejected as obsolete; source replacement, deactivation grace, and shutdown cancel page work; cancelled dirty work becomes requestable with newest input; graph-input invalidation cannot leave a stale frame current. The one cancellation in every final steady run is deadline shutdown. The two or three remaining startup rejections are bounded generation races while initial graph inputs and canonical width settle; each performed real rendering, but no steady source-completion loop remains. Aggregate reports do not assign a cause to individual run IDs.

## Graph profiles

All rows are 4.2-second release `graphs` runs without stimuli. `summary.tsv` contains every run.

| Mode | Phase | Attempts range | Accepted | Rejected range | Cancelled | User CPU range | Context switches range |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Normal | Before | 36–37 | 13–14 | 19–20 | 3 | 0.06 s | 339–385 |
| Normal | After | 8 | 4 | 3 | 1 | 0.02 s | 253–268 |
| CPU 0 | Before | 32–37 | 18–21 | 11–14 | 2–3 | 0.04–0.05 s | 336–356 |
| CPU 0 | After | 7 | 4 | 2 | 1 | 0.01 s | 238–257 |

After the fix, normal attempts fell 78% and CPU-0 attempts fell 79–81%. Normal user CPU fell from 0.06 s to 0.02 s; CPU-0 fell from 0.04–0.05 s to 0.01 s. Total voluntary plus involuntary context switches also fell in every matched mode. Maximum RSS remained noisy and neutral at 10,248–11,016 KiB normal and 10,960–11,472 KiB CPU-0. Page-render run p99 improved from 1.963–2.096 ms to 1.385–1.747 ms normal and from 1.225–1.517 ms to 0.728–0.798 ms CPU-0. Publication render p99 remained below 0.294 ms.

The worst final first paint was 104.429 ms, scheduled publication p99 2.159 ms, and shutdown 5.049 ms. Every run reported zero skipped display deadlines, zero first paints over 250 ms, zero publications over 50 ms, and `final_status: ok`.

## Event latency and validation

Three normal and three CPU-0 8.2-second `main --stimuli` runs completed all six actions in each run. The worst page, control, and config event p99 values were 51.726 ms, 52.103 ms, and 56.011 ms, with zero samples over 100 ms; all requested and observed terminal states matched. The worst first paint was 104.362 ms, scheduled publication p99 2.127 ms, and shutdown 6.171 ms, with zero skipped deadlines.

Independent verification passed focused graph-render, async-loop, scheduler, and page-render tests plus every gate in `docs/DEVELOPMENT.md`: locked fetch stability, formatting, all-target/all-feature check, Clippy with warnings denied, all-target/all-feature tests, docs, repository gate, shell syntax, user-install test, package-layout test, `git diff --check`, and touched handwritten line limits. The Qt matrix was not applicable because no render serializer, HTML, CSS, QML, or layout output changed. No plasmoidviewer or real Plasma-session workflow ran.
