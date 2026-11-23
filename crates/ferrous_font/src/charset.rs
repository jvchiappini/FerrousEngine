//! Pre-built character sets for atlas generation.
//!
//! These helpers produce `Vec<char>` ready to be passed to [`FontAtlas::new`]
//! or any of the `Font::load*` constructors.  Use the narrowest set that
//! covers your content to keep atlas size and bake time as small as possible.
//!
//! # Quick reference
//!
//! | Function | Glyphs (approx.) | When to use |
//! |---|---|---|
//! | [`ascii`] | ~95 | English-only apps |
//! | [`latin_western`] | ~300 | Western European + common symbols (`•`, `—`, `€`) |
//! | [`latin_extended`] | ~900 | Central/Eastern European + math + arrows + box drawing |
//! | [`cyrillic`] | ~350 | Russian / Slavic apps |
//!
//! ## Adding extra characters
//!
//! Use `merge` or `from_str` to combine sets:
//!
//! ```rust
//! use ferrous_font::charset;
//!
//! // All of latin_western plus a few custom symbols
//! let chars = charset::merge(&charset::latin_western(), &charset::from_str("→←↑↓★☆♥"));
//! ```

/// Basic printable ASCII (U+0020 – U+007E).
///
/// Covers English text only. No accented characters, no symbols beyond
/// standard punctuation.
pub fn ascii() -> Vec<char> {
    (' '..='~').collect()
}

/// Latin-1 Supplement (U+00A0 – U+00FF) **on top of** basic ASCII.
///
/// This single block covers the most common Western European characters:
/// - Spanish: á é í ó ú ü ñ ¡ ¿ (and uppercase variants)
/// - French: à â æ ç è ê ë î ï ô œ ù û ü ÿ
/// - German: ä ö ü ß
/// - Portuguese: ã õ
/// - Italian, Dutch, Nordic, etc.
///
/// Also includes common currency symbols (£ ¥ €), math (± ÷ ×), and
/// typographic marks (© ® ™ — – …).
pub fn latin_extended() -> Vec<char> {
    let mut chars: Vec<char> = (' '..='~').collect(); // ASCII

    // Latin-1 Supplement (all printable)
    for cp in 0x00A0u32..=0x00FFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Latin Extended-A (U+0100 – U+017F)
    // Covers ŀ Ł ł Ń ń Ņ ņ Ň ň ŉ Ŋ ŋ … all the extended letters used
    // in Polish, Czech, Slovak, Romanian, Welsh, etc.
    for cp in 0x0100u32..=0x017Fu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Latin Extended-B (U+0180 – U+024F) — Croatiam, Vietnamese, IPA, etc.
    for cp in 0x0180u32..=0x024Fu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // General Punctuation (U+2000 – U+206F)
    // em-dash — en-dash – ellipsis … curly quotes " " ' ' etc.
    for cp in 0x2000u32..=0x206Fu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Currency Symbols (U+20A0 – U+20CF) — € ₿ ₽ ₹ …
    for cp in 0x20A0u32..=0x20CFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Letterlike Symbols (U+2100 – U+214F) — ™ © ® ℃ ℉ №
    for cp in 0x2100u32..=0x214Fu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Mathematical Operators (U+2200 – U+22FF) — ∞ ≠ ≤ ≥ ± √ ∑ ∫ …
    for cp in 0x2200u32..=0x22FFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Box Drawing (U+2500 – U+257F) — useful for TUI-style UIs
    for cp in 0x2500u32..=0x257Fu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Geometric Shapes (U+25A0 – U+25FF) — ▶ ◀ ▲ ▼ ● ○ □ ■ …
    for cp in 0x25A0u32..=0x25FFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Arrows (U+2190 – U+21FF) — ← ↑ → ↓ ↔ ↕ ⇐ ⇒ …
    for cp in 0x2190u32..=0x21FFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Deduplicate (unlikely but safe)
    chars.sort_unstable();
    chars.dedup();
    chars
}

/// A focused set for UIs that need Western European languages plus common UI
/// symbols.  Smaller atlas than [`latin_extended`] but covers all everyday
/// punctuation, bullets, and typographic marks.
///
/// Includes:
/// - Basic ASCII (U+0020 – U+007E)
/// - Latin-1 Supplement (U+00A0 – U+00FF): ñ á é í ó ú ü ß ç ÿ £ ¥ © ® ×  ÷ …
/// - General Punctuation (U+2000 – U+206F): • … – — ' ' " " ‹ › «  » etc.
/// - Euro sign (U+20AC), Trade mark (U+2122)
pub fn latin_western() -> Vec<char> {
    let mut chars: Vec<char> = (' '..='~').collect(); // ASCII

    // Latin-1 Supplement (U+00A0 – U+00FF)
    for cp in 0x00A0u32..=0x00FFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // General Punctuation (U+2000 – U+206F)
    // Covers: • (U+2022), … (U+2026), – (U+2013), — (U+2014),
    //         ' ' " " (U+2018–U+201D), ‹ › (U+2039–U+203A),
    //         and all other general punctuation marks.
    for cp in 0x2000u32..=0x206Fu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }

    // Extra symbols that don't fit in the above ranges
    let extras: &[char] = &[
        '\u{20AC}', // euro sign €
        '\u{2122}', // trade mark sign ™
    ];
    chars.extend_from_slice(extras);

    chars.sort_unstable();
    chars.dedup();
    chars
}

/// Cyrillic characters (U+0400 – U+04FF) combined with ASCII.
///
/// Useful for Russian, Bulgarian, Serbian, Ukrainian, etc.
pub fn cyrillic() -> Vec<char> {
    let mut chars = ascii();
    for cp in 0x0400u32..=0x04FFu32 {
        if let Some(c) = char::from_u32(cp) {
            chars.push(c);
        }
    }
    chars.sort_unstable();
    chars.dedup();
    chars
}

/// Returns a character set built from the unique characters found in a string.
///
/// Use this when you know exactly which characters you will render and want
/// the smallest possible atlas.
///
/// ```rust
/// use ferrous_font::charset::from_str;
/// let chars = from_str("Hello, Ñoño! ¿Cómo estás?");
/// ```
pub fn from_str(s: &str) -> Vec<char> {
    let mut chars: Vec<char> = s.chars().collect();
    chars.sort_unstable();
    chars.dedup();
    chars
}

/// Merge two character sets into one (deduplicated and sorted).
pub fn merge(a: &[char], b: &[char]) -> Vec<char> {
    let mut out: Vec<char> = a.iter().chain(b.iter()).copied().collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_size() {
        assert_eq!(ascii().len(), 95);
    }

    #[test]
    fn latin_western_contains_bullet() {
        let lw = latin_western();
        assert!(
            lw.contains(&'•'),
            "latin_western() must contain U+2022 BULLET"
        );
        assert!(lw.contains(&'—'), "latin_western() must contain em-dash");
        assert!(lw.contains(&'€'), "latin_western() must contain euro sign");
        assert!(lw.contains(&'ñ'), "latin_western() must contain ñ");
        eprintln!("latin_western() = {} chars", lw.len());
    }

    #[test]
    fn latin_extended_is_superset_of_western() {
        let lw = latin_western();
        let le = latin_extended();
        for c in &lw {
            assert!(
                le.contains(c),
                "latin_extended() missing '{c}' that is in latin_western()"
            );
        }
        eprintln!("latin_extended() = {} chars", le.len());
    }

    #[test]
    fn from_str_deduplicates() {
        let chars = from_str("aabbcc");
        assert_eq!(chars, vec!['a', 'b', 'c']);
    }

    #[test]
    fn merge_combines_and_deduplicates() {
        let a = vec!['a', 'b'];
        let b = vec!['b', 'c'];
        let m = merge(&a, &b);
        assert_eq!(m, vec!['a', 'b', 'c']);
    }
}
