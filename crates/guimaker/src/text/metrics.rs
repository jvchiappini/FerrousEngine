//! Real `Inter` advances for headless text layout.
//!
//! The numbers come from `Inter-Regular.ttf` (`assets/fonts/Inter-Regular.ttf`
//! in Motioner today) through the generator described in `docs/text.md`:
//!
//! ```text
//! python scripts/gen_guimaker_metrics.py <Inter-Regular.ttf>
//! ```
//!
//! They are font units at `UPEM`, so the calculator divides by `UPEM` and
//! multiplies by the role's size. The table covers the printable ASCII range;
//! every other character falls back to [`UNKNOWN_ADVANCE`] (a half em), which
//! is deliberately visible in layout tests rather than silently guessed.
//!
//! This table is not a rasteriser and not a font: the adapter crate shapes the
//! real font in phase 5. It exists so the core's layout tests are deterministic,
//! GPU-free and still reflect the typeface a user will see (G-0001 §3).

/// Font units per em of the shipped Inter faces.
pub(crate) const UPEM: f32 = 2048.0;
/// Ascender in font units.
pub(crate) const ASCENT: f32 = 1984.0;
/// Descender in font units (negative).
pub(crate) const DESCENT: f32 = -494.0;
/// Advance of `U+2026 HORIZONTAL ELLIPSIS`, in font units.
pub(crate) const ELLIPSIS: u16 = 1770;
/// Advance of `U+2013 EN DASH`, in font units.
pub(crate) const EN_DASH: u16 = 1024;
/// Advance of `U+2014 EM DASH`, in font units.
pub(crate) const EM_DASH: u16 = 2048;
/// Advance of `U+2019 RIGHT SINGLE QUOTATION MARK`, in font units.
pub(crate) const RIGHT_QUOTE: u16 = 534;
/// Advance used for a glyph that is not in the table, in font units (half em).
pub(crate) const UNKNOWN_ADVANCE: u16 = 1024;
/// Fixed advance for tabular digits, in font units.
pub(crate) const TABULAR_DIGIT: u16 = 1270;
/// The first code point of [`ASCII_ADVANCES`].
pub(crate) const ASCII_FIRST: u32 = 0x20;
/// Number of printable ASCII advances.
pub(crate) const ASCII_COUNT: usize = 95;

/// Advance widths of printable ASCII (`0x20..=0x7E`), in font units.
pub(crate) const ASCII_ADVANCES: [u16; ASCII_COUNT] = [
    576, 589, 954, 1297, 1314, 2011, 1319, 614, 747, 747, 1026, 1355, 590, 942, 590, 738, 1292,
    833, 1249, 1265, 1323, 1215, 1270, 1159, 1267, 1270, 590, 618, 1355, 1355, 1355, 1047, 1978,
    1413, 1340, 1496, 1478, 1231, 1209, 1528, 1522, 550, 1169, 1376, 1158, 1850, 1543, 1566, 1308,
    1566, 1318, 1314, 1322, 1524, 1413, 2018, 1397, 1390, 1288, 747, 738, 747, 965, 934, 661, 1150,
    1254, 1170, 1254, 1194, 758, 1256, 1211, 496, 496, 1124, 496, 1794, 1210, 1228, 1254, 1254,
    771, 1081, 670, 1211, 1151, 1676, 1118, 1151, 1131, 873, 681, 873, 1355,
];

/// The advance of one character, in font units.
pub(crate) fn advance_units(ch: char, tabular: bool) -> u16 {
    if ch.is_ascii_digit() && tabular {
        return TABULAR_DIGIT;
    }
    match ch {
        '\u{2026}' => ELLIPSIS,
        '\u{2013}' => EN_DASH,
        '\u{2014}' => EM_DASH,
        '\u{2019}' | '\u{2018}' => RIGHT_QUOTE,
        '\u{0020}'..='\u{007E}' => {
            let index = (ch as u32 - ASCII_FIRST) as usize;
            ASCII_ADVANCES
                .get(index)
                .copied()
                .unwrap_or(UNKNOWN_ADVANCE)
        }
        _ => UNKNOWN_ADVANCE,
    }
}

#[cfg(test)]
mod tests {
    use super::{advance_units, ASCII_COUNT, ASCII_FIRST, ELLIPSIS};

    #[test]
    fn the_ascii_table_matches_its_bounds() {
        assert_eq!(ASCII_FIRST, 0x20);
        assert_eq!(ASCII_COUNT, 0x7F - 0x20);
        assert!(super::ASCII_ADVANCES.iter().all(|advance| *advance > 0));
    }

    #[test]
    fn known_glyphs_resolve() {
        assert_eq!(advance_units(' ', false), 576);
        assert_eq!(advance_units('I', false), 550);
        assert_eq!(advance_units('\u{2026}', false), ELLIPSIS);
        assert!(advance_units('中', false) > 0);
    }
}
