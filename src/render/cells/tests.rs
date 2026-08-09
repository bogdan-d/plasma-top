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
