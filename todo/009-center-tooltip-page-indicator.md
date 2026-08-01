# Center the tooltip page indicator

## Status

Visual regression observed in the live three-page tooltip.

## Problem

The three pager dots at the bottom of the tooltip no longer appear horizontally centered. Pager HTML currently estimates leading non-breaking spaces from a character width, while the main page and deeper pages can use different width inputs. The connection to the wheel failure is unproven; page selection and pager rendering share state but use separate code paths.

## Relevant files

- `src/page_commands.rs`
- `src/daemon.rs`
- `src/render/mono.rs`
- `style/style-dark.css`
- `style/style-light.css`
- `plasmoid/package/contents/ui/main.qml`
- `docs/LAYOUT.md`

## Handoff

1. Capture the offset on every active page and record tooltip width, font family, font size, scale factor, and theme.
2. Check whether the error comes from the width passed to `pager_html`, the monospace-space calculation, Qt RichText handling, or popup padding.
3. Keep the active-page marker and page count correct while changing alignment.
4. Mirror any CSS change between dark and light styles and do not introduce HTML tables.
5. Validate with real Qt RichText and the live Plasma tooltip.

## Done when

- Pager dots are visually centered on every page at supported widths and common scale factors.
- Current-page highlighting remains correct while paging in both directions.
- Focused pager tests, Qt visual checks, QML smoke, and full gates pass.

