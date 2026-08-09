use super::*;

#[test]
fn strip_html_preserves_rows_and_spacing() {
    assert_eq!(
        strip_html("<style>.x{}</style><div>a&nbsp;b<br>c</div>"),
        "a b\nc\n"
    );
}
