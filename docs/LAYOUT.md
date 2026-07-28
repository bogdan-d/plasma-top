# Layout types (row layouts)

The monospace serializer ([rust/src/render/mono.rs](../rust/src/render/mono.rs)) doesn't have a layout for every cell count: it reduces everything to **five layout plans (the `Plan` variants selected by `plan_row`), and every row maps onto one of them based on its *shape*, not its cell count.

## The rules that govern everything

1. **Shape decides, not cell count.** What matters is how the last cell is aligned. Last one right-aligned → `rightval`; but if it's a 3-cell row with the middle cell marked `center` → `centermid` (the middle one centers, see below). Four cells with the values in position 1 and 3 right-aligned → `twopair`. A single cell → `center` if a title, `left` otherwise.
2. **Automatic, flush columns.** Every group of identically-shaped rows (a block) measures its own widths (the max per column) and the columns sit flush: if you want a gap, put it in the text (e.g. the label's `:`).
3. **A single right edge for everything.** The widest row across the entire surface is measured (`global_width`) and every value aligns to that edge, so percentages stay column-aligned even across different blocks.

There are three kinds of spacing: inside a cell (`pad_left`/`pad_right`, to detach a spark), between columns (alignment padding, a `.gap` span), and between blocks (the small or big separator `<div>`).

## Schema for each type

### 1 cell — `center` (titles)

```
│            ━━━ Cpu & Mem ━━━            │
└─ padding ─┘                 └─ padding ─┘
   text centered over the total width
```

### 1 cell — `left` (standalone spark / bars)

```
│ ▁▂▃▅▇█▇▅                                │
└─ everything left-aligned, fills what's needed
```

Same single-cell shape: the inline bars (`*_bar`) and the single-glyph vertical column (`*_column` — an eighth-block char `▁..█` that grows upward, for the horizontal panel; height in `[column_panel]`, background/palette in `style-dark.css`). Which of the two goes in the panel is decided by orientation, via the `[panel_horizontal]`/`[panel_vertical]` overrides (see README).

### 2 cells — `rightval` (label + value, the most common case)

```
│ Cpu:                              45%  │
└┬─┘└──────── padding gap ────────┘└┬─┘
 │                                  └─ value, aligned to the right edge
 └─ label, left-aligned
```

### 3 cells — `rightval` (label + extra + value, the spark combos)

```
│ Cpu  ▁▂▃▅▇                        45%  │
└┬─┘  └─┬─┘└──── padding gap ─────┘└┬─┘
 │      │                          └─ value, at the right edge
 │      └─ extra (spark), its own column
 └─ label, its own column
```

Label and extra are padded to their own columns; only the value floats to the right edge.

### 3 cells — `centermid` (disk_usage: label + used/total + value)

```
│ 󰄫 Backup:     504G / 838G      63%  │
│ 󰄫 Root:        56G / 77G       76%  │
└┬───────┘   └─────┬─────┘     └┬─┘
 │                 │            └─ value (%), at the right edge
 │                 └─ used/total block centered between label and value,
 │                    "used" right-aligned and "total" left-aligned → the `/` stays column-aligned
 └─ label, its own column
```

Variant of `rightval` for when the middle cell asks for `align="center"` (only `disk_usage`): instead of sitting flush against the label, it **centers** in the space between the label column and the value zone. Two details keep it clean: (a) the `disk_space` cell right-aligns "used" and left-aligns "total" to widths shared across all disks, so every cell has the same width and the `/` lands on the same column; (b) centering is measured against the **block's value-column width** (not the single row's), so a wider `100%` elsewhere doesn't shift the slashes on the other rows.

### 4 cells — `twopair` (net_speed, disk_io: two pairs on the same row)

```
│ ↑ 12M               ↓ 3M               │
└──── left half ──────┘└──── right half ─┘
  label+value              label+value
  value at half's edge     value at half's edge
```

The total width is split in two, each pair lives in its own half.

### 4 cells — `left` (live_history, cpu_mem spark: NOT twopair)

```
│ Cpu ▇▇▇▅▃  Mem ▃▅▇█▇                    │
└─ the even cells are left-aligned bars/sparks, no right edge
```

The difference between the last two — same cell count, opposite layout — is purely alignment: if cells 1 and 3 are right-aligned values it's `twopair`, if they're left-aligned bars it's `left`.

## Who builds the rows

The **layout generator is one single thing** (`plan_row`/`Plan::emit` in `rust/src/render/mono.rs`): no item chooses its own layout, it only produces `Cell`/`Row` and the layout is inherited from the shape. What *builds* those rows, though, is distinct, across three levels:

- **Regular (declarative)** — standard-shape items (most of them) are *data* in [rust/src/render/registry.rs](../rust/src/render/registry.rs), keyed by the validated metric/form identity and composed with helpers from [rust/src/render/cells.rs](../rust/src/render/cells.rs). Adding a regular item is a small dispatch entry.
- **Irregular (exception functions)** — cases with their own logic (net/wifi joins, batteries, top_process) are explicit `PanelFormatter` methods in [rust/src/render/formatter.rs](../rust/src/render/formatter.rs), reached through the same dispatch path. The bar/column/spark/braille forms — the "own-skeleton" percentage encodings — live together in [rust/src/render/traces.rs](../rust/src/render/traces.rs).
- **Shared helpers** — when several irregular ones share the *same* layout (not just different data), the logic lives in one helper. Pair forms share the same paired-grid path; net speed and disk I/O share `render_dual_rate_rows`. Callers pass only their metric-specific differences.

The boundary is deliberate: only **structural** duplication (the same layout logic repeated) is centralized, not legitimate variety. `render/traces.rs` collapses the bar/column/spark/braille duplication onto one combined-row skeleton and one standalone builder; the plain label+value rows stay as distinct methods — their logic is genuinely different, and a parametric helper would cost more clarity than it saves.
