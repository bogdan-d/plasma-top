use super::*;

#[test]
fn wrap_visible_leaves_lines_within_width_untouched() {
    let line = "\u{1b}[1mCPU\u{1b}[m: Ryzen";
    assert_eq!(wrap_visible(line, 10), line);
}

#[test]
fn wrap_visible_breaks_at_words_with_indent_and_keeps_sgr_in_order() {
    let line = "\u{1b}[36mDisk\u{1b}[m: 1.82 TiB (\u{1b}[32m16%\u{1b}[m) - btrfs";
    assert_eq!(
        wrap_visible(line, 12),
        "\u{1b}[36mDisk\u{1b}[m: 1.82\n  TiB (\u{1b}[32m16%\u{1b}[m)\n  - btrfs"
    );
}

#[test]
fn wrap_visible_breaks_a_word_longer_than_the_line() {
    assert_eq!(
        wrap_visible("ab /var/mnt/xdata", 8),
        "ab\n  /var/m\n  nt/xda\n  ta"
    );
}
