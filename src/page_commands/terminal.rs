//! Terminal output cleanup for command pages: SGR color sequences survive for the applet's ANSI renderer, everything else is dropped.

pub(super) fn strip_terminal_noise(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                index += 1;
            }
            0x1b => {
                if let Some(next) = bytes.get(index + 1).copied() {
                    match next {
                        b']' => {
                            index = skip_osc(bytes, index + 2);
                        }
                        b'[' => {
                            let (next_index, keep) = skip_csi(bytes, index);
                            if keep {
                                out.extend_from_slice(&bytes[index..next_index]);
                            }
                            index = next_index;
                        }
                        b'=' | b'>' | b'N' | b'O' | b'P' | b'X' | b'^' | b'_' | b'c' => {
                            index += 2;
                        }
                        _ => {
                            out.push(bytes[index]);
                            index += 1;
                        }
                    }
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn skip_osc(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        match bytes[index] {
            0x07 => return index + 1,
            0x1b if bytes.get(index + 1) == Some(&b'\\') => return index + 2,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn skip_csi(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut index = start + 2;
    while index < bytes.len() {
        let byte = bytes[index];
        if (0x40..=0x7e).contains(&byte) {
            return (index + 1, byte == b'm');
        }
        index += 1;
    }
    (bytes.len(), false)
}

pub(super) fn strip_sgr(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            let (next_index, keep) = skip_csi(bytes, index);
            if keep {
                index = next_index;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Continuation lines are indented so a wrapped value reads as part of its line.
const WRAP_INDENT: usize = 2;

/// Word-wraps `line` to `width` visible characters; a word longer than a whole line is broken where it overflows.
///
/// SGR sequences stay in stream order, so the applet's color state carries across the inserted breaks.
pub(super) fn wrap_visible(line: &str, width: usize) -> String {
    if width == 0 || strip_sgr(line).chars().count() <= width {
        return line.to_owned();
    }
    let indent = WRAP_INDENT.min(width / 2);
    let mut out = String::with_capacity(line.len() + 8);
    let mut col = 0;
    for (index, (word, word_width)) in split_words(line).into_iter().enumerate() {
        if index > 0 {
            if col + 1 + word_width > width {
                break_line(&mut out, &mut col, indent);
            } else {
                out.push(' ');
                col += 1;
            }
        }
        push_word(&mut out, &word, &mut col, width, indent);
    }
    out
}

/// Splits on spaces, keeping each SGR sequence attached to the word it precedes or ends.
fn split_words(line: &str) -> Vec<(String, usize)> {
    let bytes = line.as_bytes();
    let mut words = Vec::new();
    let mut current = (String::new(), 0);
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            let (next_index, keep_sgr) = skip_csi(bytes, index);
            if keep_sgr {
                current.0.push_str(&line[index..next_index]);
                index = next_index;
                continue;
            }
        }
        let Some(ch) = line[index..].chars().next() else {
            break;
        };
        index += ch.len_utf8();
        if ch == ' ' {
            words.push(std::mem::take(&mut current));
        } else {
            current.0.push(ch);
            current.1 += 1;
        }
    }
    words.push(current);
    words
}

fn push_word(out: &mut String, word: &str, col: &mut usize, width: usize, indent: usize) {
    let bytes = word.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            let (next_index, keep_sgr) = skip_csi(bytes, index);
            if keep_sgr {
                out.push_str(&word[index..next_index]);
                index = next_index;
                continue;
            }
        }
        let Some(ch) = word[index..].chars().next() else {
            break;
        };
        if *col >= width {
            break_line(out, col, indent);
        }
        out.push(ch);
        *col += 1;
        index += ch.len_utf8();
    }
}

fn break_line(out: &mut String, col: &mut usize, indent: usize) {
    out.push('\n');
    out.push_str(&" ".repeat(indent));
    *col = indent;
}

#[cfg(test)]
mod tests;
