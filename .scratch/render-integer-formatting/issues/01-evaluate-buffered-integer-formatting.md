# 01: Evaluate buffered integer formatting for display publication

**What to build:** Determine whether Rust 1.98's buffered integer formatting materially reduces the cost of publishing panel and tooltip HTML. Adopt it where a measured gain warrants the change while preserving identical display output, or record the evidence for keeping the current formatting.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

- [ ] Measure representative panel and presented tooltip rendering, including the time or allocations attributable to integer formatting, and record a reproducible baseline.
- [ ] Compare the current approach with `format_into` and `NumBuffer` using the same readings, including boundary values, and report the effect on complete publication work.
- [ ] If the measured benefit warrants adoption, update the relevant rendering path and verify byte-for-byte HTML compatibility, canonical tooltip width coverage, and the required repository checks.
- [ ] If the benefit does not warrant adoption, record the measurements and conclusion without changing production rendering.
