//! Grapheme, word and line boundaries over a `&str`.
//!
//! Everything the text editor moves or deletes is a grapheme cluster, never a
//! `char`: deleting once must remove a whole emoji (including a ZWJ family or a
//! skin-tone modifier) and a combining accent must travel with its base. The
//! cluster boundaries come from `unicode-segmentation` (the dependency is
//! recorded in `G-0004`); this module adds the word and line helpers the
//! editor needs.
//!
//! The walking functions are `O(clusters)` in the prefix they cover and never
//! allocate. Byte offsets that are not on a boundary are rounded to the
//! containing cluster.

use unicode_segmentation::UnicodeSegmentation;

/// A byte offset rounded down to a grapheme boundary.
#[must_use]
pub fn floor_boundary(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    let mut last = 0;
    for (offset, _) in text.grapheme_indices(true) {
        if offset > byte {
            break;
        }
        last = offset;
    }
    last
}

/// A byte offset rounded up to a grapheme boundary.
#[must_use]
pub fn ceil_boundary(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    for (offset, grapheme) in text.grapheme_indices(true) {
        if offset >= byte {
            return offset;
        }
        if offset + grapheme.len() > byte {
            return offset + grapheme.len();
        }
    }
    text.len()
}

/// The previous grapheme boundary before `byte`.
#[must_use]
pub fn previous(text: &str, byte: usize) -> usize {
    let byte = floor_boundary(text, byte.min(text.len()));
    let mut previous = 0;
    for (offset, _) in text.grapheme_indices(true) {
        if offset >= byte {
            break;
        }
        previous = offset;
    }
    previous
}

/// The next grapheme boundary after `byte`.
#[must_use]
pub fn next(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    ceil_boundary(text, byte.saturating_add(1))
}

/// The number of grapheme clusters in `text`.
#[must_use]
pub fn count(text: &str) -> usize {
    text.graphemes(true).count()
}

/// Fills `out` with every grapheme boundary, including `0` and `text.len()`.
///
/// The buffer is reused; a steady frame does not allocate.
pub fn boundaries(text: &str, out: &mut Vec<usize>) {
    out.clear();
    out.reserve(text.len().min(64));
    out.push(0);
    for (offset, grapheme) in text.grapheme_indices(true) {
        let end = offset + grapheme.len();
        if end > *out.last().unwrap_or(&0) {
            out.push(end);
        }
    }
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The previous word boundary before `byte` (Ctrl+Left / Ctrl+Backspace).
///
/// Whitespace and punctuation are skipped, then the whole word before them.
#[must_use]
pub fn previous_word(text: &str, byte: usize) -> usize {
    let mut cursor = floor_boundary(text, byte.min(text.len()));
    while cursor > 0 {
        let start = previous(text, cursor);
        let segment = &text[start..cursor];
        if segment.chars().any(|ch| !is_word(ch)) {
            cursor = start;
        } else {
            break;
        }
    }
    while cursor > 0 {
        let start = previous(text, cursor);
        if text[start..cursor].chars().any(is_word) {
            cursor = start;
        } else {
            break;
        }
    }
    cursor
}

/// The next word boundary after `byte` (Ctrl+Right / Ctrl+Delete).
#[must_use]
pub fn next_word(text: &str, byte: usize) -> usize {
    let mut cursor = ceil_boundary(text, byte.min(text.len()));
    while cursor < text.len() {
        let end = next(text, cursor);
        if text[cursor..end].chars().any(|ch| !is_word(ch)) {
            cursor = end;
        } else {
            break;
        }
    }
    while cursor < text.len() {
        let end = next(text, cursor);
        if text[cursor..end].chars().any(is_word) {
            cursor = end;
        } else {
            break;
        }
    }
    cursor
}

/// The word around `byte`, or the grapheme containing it when it is not in a
/// word (double-click).
#[must_use]
pub fn word_at(text: &str, byte: usize) -> (usize, usize) {
    let start = floor_boundary(text, byte.min(text.len()));
    if start == text.len() {
        return (start, start);
    }
    let end = next(text, start);
    let grapheme = &text[start..end];
    if !grapheme.chars().any(is_word) {
        return (start, end);
    }
    let mut word_start = start;
    while word_start > 0 {
        let candidate = previous(text, word_start);
        if text[candidate..word_start].chars().any(is_word) {
            word_start = candidate;
        } else {
            break;
        }
    }
    let mut word_end = end;
    while word_end < text.len() {
        let candidate = next(text, word_end);
        if text[word_end..candidate].chars().any(is_word) {
            word_end = candidate;
        } else {
            break;
        }
    }
    (word_start, word_end)
}

/// The line around `byte`, excluding the newline (triple-click, Home/End).
#[must_use]
pub fn line_at(text: &str, byte: usize) -> (usize, usize) {
    let byte = floor_boundary(text, byte.min(text.len()));
    let start = text[..byte].rfind('\n').map_or(0, |index| index + 1);
    let end = text[byte.min(text.len())..]
        .find('\n')
        .map_or(text.len(), |index| byte + index);
    (start, end)
}

/// The start of every line, plus `text.len()` as the final entry.
///
/// `O(text.len())`; the editor caches the result until the text changes.
pub fn line_starts(text: &str, out: &mut Vec<usize>) {
    out.clear();
    out.push(0);
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            out.push(index + 1);
        }
    }
    if out.last().copied() != Some(text.len()) {
        out.push(text.len());
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::*;

    #[test]
    fn grapheme_units_are_whole_clusters() {
        let text = "a\u{0301}👩‍👩‍👧‍👦🇦🇷🇧🇷b";
        assert_eq!(count(text), 5);
        let end = next(text, 0);
        assert_eq!(&text[..end], "a\u{0301}");
        let after_emoji = next(text, end);
        assert_eq!(&text[end..after_emoji], "👩‍👩‍👧‍👦");
        let last = previous(text, text.len());
        assert_eq!(&text[last..], "b");
    }

    #[test]
    fn boundaries_include_both_ends() {
        let mut out = Vec::new();
        boundaries("naïve", &mut out);
        assert_eq!(out, vec![0, 1, 2, 4, 5, 6]);
        boundaries("", &mut out);
        assert_eq!(out, vec![0]);
    }

    #[test]
    fn off_boundary_offsets_round() {
        let text = "a\u{0301}b";
        assert_eq!(floor_boundary(text, 1), 0);
        assert_eq!(ceil_boundary(text, 1), 3);
        assert_eq!(previous(text, 1), 0);
        assert_eq!(next(text, 0), 3);
    }

    #[test]
    fn word_movement_skips_then_takes_the_word() {
        let text = "hello, bigger world";
        assert_eq!(previous_word(text, text.len()), 14);
        assert_eq!(previous_word(text, 13), 7);
        assert_eq!(previous_word(text, 7), 0);
        assert_eq!(next_word(text, 0), 5);
        assert_eq!(next_word(text, 5), 13);
        assert_eq!(next_word(text, 13), text.len());
    }

    #[test]
    fn words_include_digits_and_underscores() {
        let text = "a_b + 42";
        assert_eq!(word_at(text, 1), (0, 3));
        assert_eq!(word_at(text, 7), (6, 8));
        assert_eq!(word_at(text, 3), (3, 4));
    }

    #[test]
    fn an_emoji_is_a_word_of_its_own_for_double_click() {
        let text = "hi 👩‍👩‍👧‍👦 there";
        let (start, end) = word_at(text, 4);
        assert_eq!(&text[start..end], "👩‍👩‍👧‍👦");
    }

    #[test]
    fn line_bounds_ignore_newlines() {
        let text = "one\ntwo\nthree";
        assert_eq!(line_at(text, 1), (0, 3));
        assert_eq!(line_at(text, 5), (4, 7));
        assert_eq!(line_at(text, 9), (8, 13));
        assert_eq!(line_at(text, text.len()), (8, 13));
    }

    #[test]
    fn line_starts_cover_every_line() {
        let mut out = Vec::new();
        line_starts("one\ntwo\n", &mut out);
        assert_eq!(out, vec![0, 4, 8]);
        line_starts("", &mut out);
        assert_eq!(out, vec![0]);
    }

    #[test]
    fn cjk_and_accents_move_by_cluster() {
        let text = "日本語 café";
        assert_eq!(count(text), 8);
        assert_eq!(next(text, 0), 3);
        assert_eq!(previous_word(text, text.len()), 10);
    }
}
