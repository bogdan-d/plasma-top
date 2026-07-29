//! Shared formatter helpers and light cell-building utilities.

use toml::{Table, Value};

use crate::config::Config;

use super::model::{Cell, Entry, Ident, Separator, SeparatorSize, non_breaking_spaces};

pub(crate) const TEMP_SCALE: &str = "C";
pub(crate) const DISK_LABEL_MAX: usize = 12;
pub(crate) const SSID_MAX: usize = 16;
pub(crate) const NETDEV_MAX: usize = 12;
pub(crate) const BATTERY_ALTERNATE_SECONDS: u64 = 5;

pub(crate) const TOP_PROCESS_MIN_PID: usize = 6;
pub(crate) const TOP_PROCESS_COMM_MIN: usize = 15;
pub(crate) const TOP_PROCESS_SIDE_COLS: usize = 2 + 1 + 4 + 1 + 5;
pub(crate) const TOP_PROCESS_MIN_WIDTH: usize =
    TOP_PROCESS_MIN_PID + TOP_PROCESS_COMM_MIN + TOP_PROCESS_SIDE_COLS;

pub(crate) fn table_text<'a>(table: &'a Table, key: &str) -> &'a str {
    table.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(crate) fn label_cell(
    cfg: &Config,
    metric: &str,
    form: Option<&str>,
    tooltip: bool,
    text: Option<&str>,
    glyph: Option<&str>,
) -> Cell {
    let glyph = glyph.unwrap_or_else(|| table_text(&cfg.icons, metric));
    let css_class = format!("label {}", Ident::new(metric, form).css());
    if !tooltip {
        return Cell::classified(glyph, css_class);
    }

    let word = text.unwrap_or_else(|| table_text(&cfg.labels, metric));
    let delimiter = table_text(&cfg.labels, "delimiter");
    Cell::classified(format!("{glyph} {word}{delimiter}"), css_class)
}

pub(crate) fn regular_label_cell(
    cfg: &Config,
    metric: &str,
    form: Option<&str>,
    tooltip: bool,
    text: Option<&str>,
    glyph: Option<&str>,
) -> Option<Cell> {
    if !tooltip && !cfg.panel.glyphs {
        None
    } else {
        Some(label_cell(cfg, metric, form, tooltip, text, glyph))
    }
}

pub(crate) fn net_fmt(bps: u64) -> String {
    let kilobytes = bps / 1000;
    if kilobytes >= 1000 {
        format!("{}M", kilobytes / 1000)
    } else if kilobytes > 0 {
        format!("{kilobytes}K")
    } else {
        String::from("0")
    }
}

pub(crate) fn middle_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    if max_chars <= 1 {
        return String::from("…");
    }

    let chars: Vec<char> = text.chars().collect();
    let keep = max_chars - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;

    let mut output = String::with_capacity(max_chars);
    output.extend(chars.iter().take(head));
    output.push('…');
    if tail > 0 {
        output.extend(chars.iter().skip(chars.len() - tail));
    }
    output
}

pub(crate) fn disk_label(mount: &str) -> String {
    if mount == "/" {
        return String::from("Root");
    }

    let mut label = mount
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(mount);
    if label.is_empty() {
        label = mount;
    }

    let mut chars: Vec<char> = label.chars().collect();
    if chars.len() > DISK_LABEL_MAX {
        chars.truncate(DISK_LABEL_MAX - 1);
        chars.push('…');
    }
    if let Some(first) = chars.first_mut() {
        first.make_ascii_uppercase();
    }
    chars.into_iter().collect()
}

pub(crate) fn hd_label(label: &str) -> String {
    let base = if let Some(controller) = label
        .strip_prefix("nvme")
        .and_then(|rest| rest.split_once('n').map(|(head, _)| format!("nvme{head}")))
    {
        controller
    } else {
        label.to_owned()
    };

    let trimmed = base.trim_end_matches(|c: char| c.is_ascii_digit());
    let mut chars: Vec<char> = trimmed.chars().collect();
    if let Some(first) = chars.first_mut() {
        first.make_ascii_uppercase();
    }
    chars.into_iter().collect()
}

pub(crate) fn fmt_freq(mhz: Option<f64>, tooltip: bool) -> String {
    let Some(mhz) = mhz else {
        return String::from(super::model::EMPTY_VALUE);
    };
    if mhz >= 1000.0 {
        if tooltip {
            format!("{:.1} GHz", mhz / 1000.0)
        } else {
            format!("{:.1}", mhz / 1000.0)
        }
    } else if tooltip {
        format!("{} MHz", mhz as i32)
    } else {
        format!("{}", mhz as i32)
    }
}

pub(crate) fn fmt_disk_space(
    used_gib: Option<u64>,
    total_gib: Option<u64>,
    used_class: Option<&str>,
    used_width: usize,
    total_width: usize,
) -> String {
    let (Some(used_gib), Some(total_gib)) = (used_gib, total_gib) else {
        return String::new();
    };

    let used_text = format!("{used_gib}G");
    let total_text = format!("{total_gib}G");
    let used_html = used_class.map_or_else(
        || used_text.clone(),
        |class| format!(r#"<span class="{class}">{used_text}</span>"#),
    );
    format!(
        "{}{} / {}{}",
        non_breaking_spaces(used_width.saturating_sub(used_text.chars().count())),
        used_html,
        total_text,
        non_breaking_spaces(total_width.saturating_sub(total_text.chars().count())),
    )
}

pub(crate) fn separator_size(name: &str) -> Option<SeparatorSize> {
    match name {
        "separator_small" => Some(SeparatorSize::Small),
        "separator_big" => Some(SeparatorSize::Big),
        _ => None,
    }
}

pub(crate) fn normalize_separators(entries: Vec<Entry>) -> Vec<Entry> {
    fn rank(size: SeparatorSize) -> u8 {
        match size {
            SeparatorSize::Small => 0,
            SeparatorSize::Big => 1,
        }
    }

    let mut out = Vec::with_capacity(entries.len());
    let mut pending: Option<SeparatorSize> = None;
    for entry in entries {
        match entry {
            Entry::Separator(separator) => {
                if pending.is_none_or(|current| rank(separator.size) > rank(current)) {
                    pending = Some(separator.size);
                }
            }
            Entry::Row(row) => {
                if let Some(size) = pending.take().filter(|_| !out.is_empty()) {
                    out.push(Entry::Separator(Separator { size }));
                }
                out.push(Entry::Row(row));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::model::{Entry, value_cell};

    fn row(text: &str) -> Entry {
        Entry::Row(vec![value_cell(text, None, None, 0)])
    }

    #[test]
    fn middle_ellipsis_keeps_head_and_tail() {
        assert_eq!(middle_ellipsis("abcdefgh", 6), "abc…gh");
    }

    #[test]
    fn disk_and_hd_labels_match_python_helpers() {
        assert_eq!(disk_label("/"), "Root");
        assert_eq!(disk_label("/mnt/data"), "Data");
        assert_eq!(hd_label("nvme0n1"), "Nvme");
        assert_eq!(hd_label("sda"), "Sda");
    }

    #[test]
    fn normalize_separators_drops_edges_and_keeps_largest_gap() {
        let out = normalize_separators(vec![
            Entry::Separator(Separator {
                size: SeparatorSize::Small,
            }),
            row("a"),
            Entry::Separator(Separator {
                size: SeparatorSize::Small,
            }),
            Entry::Separator(Separator {
                size: SeparatorSize::Big,
            }),
            row("b"),
            Entry::Separator(Separator {
                size: SeparatorSize::Big,
            }),
        ]);

        assert_eq!(
            out,
            vec![
                row("a"),
                Entry::Separator(Separator {
                    size: SeparatorSize::Big,
                }),
                row("b"),
            ]
        );
    }
}
