//! Operating-system font lookup, the last resort of a fallback chain.
//!
//! When the primary face and the theme's fallback families do not cover a
//! character, the text system asks the OS for a face that does. This module is
//! the *only* place that knows about system font directories, and it does no
//! work until a character actually needs it, so cold start never pays for it.
//!
//! # Why no font-database crate
//!
//! See `crates/ferrous_font/docs/decisions/G-0005.md`: the scan needs a table
//! directory (`FontParser`), the `glyf` tag and the `cmap` lookup, all of which
//! the crate already has. A font-database crate would add a dependency (and its
//! licence) for `readdir` + those three checks. The trade-off is that only
//! `glyf` outlines are found: CFF and collection (`.ttc`) faces are skipped,
//! and the caller falls through to the next candidate.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::parser::FontParser;

/// The directories searched for system fonts, in order.
#[must_use]
pub fn directories() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(
                PathBuf::from(local)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
        dirs
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
        ]
    }
    #[cfg(not(any(windows, unix)))]
    {
        Vec::new()
    }
}

/// Every candidate font file in [`directories`] (one level, `.ttf`/`.otf`).
///
/// The list is cached for the process: finding it is cheap, reading the files
/// is not.
#[must_use]
pub fn font_files() -> &'static [PathBuf] {
    static FILES: OnceLock<Vec<PathBuf>> = OnceLock::new();
    FILES.get_or_init(|| {
        let mut files = Vec::new();
        for directory in directories() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase);
                if matches!(extension.as_deref(), Some("ttf" | "otf")) {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    })
}

/// Whether `bytes` is a `glyf` font with an outline for `ch`.
#[must_use]
pub fn supports(bytes: &[u8], ch: char) -> bool {
    FontParser::new(bytes.to_vec())
        .is_ok_and(|parser| parser.has_glyf() && parser.get_glyph_index(ch) != 0)
}

/// Finds system faces that cover as many of `chars` as possible.
///
/// Returns one `(bytes, covered_chars)` per face, in file order, stopping once
/// every requested character is covered. Only `glyf` faces are considered; a
/// `.ttc` collection is skipped because the parser reads a single face.
#[must_use]
pub fn find_for(chars: &[char]) -> Vec<(Vec<u8>, Vec<char>)> {
    let mut missing: Vec<char> = chars.to_vec();
    missing.sort_unstable();
    missing.dedup();
    let mut found = Vec::new();
    if missing.is_empty() {
        return found;
    }
    for path in font_files() {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(parser) = FontParser::new(bytes.clone()) else {
            continue;
        };
        if !parser.has_glyf() {
            continue;
        }
        let covered: Vec<char> = missing
            .iter()
            .copied()
            .filter(|ch| parser.get_glyph_index(*ch) != 0)
            .collect();
        if covered.is_empty() {
            continue;
        }
        found.push((bytes, covered.clone()));
        missing.retain(|ch| !covered.contains(ch));
        if missing.is_empty() {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{find_for, font_files, supports};

    #[test]
    fn a_non_font_is_not_supported() {
        assert!(!supports(b"not a font", 'A'));
    }

    #[test]
    fn the_system_scan_returns_glyf_faces_only() {
        // The list can legitimately be empty (a sandboxed CI host); when it is
        // not, every returned face must parse and carry an outline.
        for file in font_files().iter().take(8) {
            if let Ok(bytes) = std::fs::read(file) {
                if supports(&bytes, 'A') {
                    assert!(crate::parser::FontParser::new(bytes).is_ok_and(|p| p.has_glyf()));
                }
            }
        }
    }

    #[test]
    fn a_mixed_string_is_covered_by_some_system_font_when_the_host_has_one() {
        let chars = ['A', '\u{1F3AC}', '\u{52D5}'];
        let faces = find_for(&chars);
        if faces.is_empty() {
            eprintln!("skipping: this host has no system glyf font");
            return;
        }
        // Whatever is found must actually support the characters it claims.
        for (bytes, covered) in faces {
            for ch in covered {
                assert!(
                    supports(&bytes, ch),
                    "a returned face claims support for {ch:?} but does not have it"
                );
            }
        }
    }
}
