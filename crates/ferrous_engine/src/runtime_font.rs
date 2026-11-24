//! The runtime glyph atlas, with lazy fallback for characters it lacks.
//!
//! The UI atlas starts as the app's font over `latin_western`. When a frame's
//! text uses a character the atlas does not have (CJK, Arabic, symbols), the
//! host requests it through `AppContext::request_glyphs`; the runner re-bakes
//! **one composite atlas** (`BakedAtlas::build_multi` + `system::find_for`) and
//! re-binds it before that same frame's `draw_ui`, so the frame that is painted
//! always matches the atlas it encodes with.
//!
//! Nothing here runs for ASCII-only text: a requested character that the atlas
//! already has is dropped before the OS font scan, so an ASCII app never opens
//! a fallback face.

use std::sync::Arc;

use ferrous_font::{charset, system, Font};

/// The app's composite UI atlas plus the state needed to extend it.
pub struct RuntimeFont {
    /// The app face (the bytes the runner was configured with).
    bytes: Vec<u8>,
    /// Every character currently baked, so a re-bake never loses a glyph.
    chars: Vec<char>,
    font: Arc<Font>,
    bakes: u32,
    /// Characters no face covers. They draw `.notdef` and are never requested
    /// again: without this set, an uncovered character would re-scan the OS
    /// fonts and re-bake the atlas on every frame, and the app would never idle.
    unresolved: Vec<char>,
}

impl RuntimeFont {
    /// Builds the initial atlas over `latin_western`.
    #[must_use]
    pub fn new(bytes: &[u8], device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let chars = charset::latin_western();
        let font = Font::try_load_bytes_cached(bytes, device, queue, chars.clone(), None).ok()?;
        Some(Self {
            bytes: bytes.to_vec(),
            chars,
            font: Arc::new(font),
            bakes: 0,
            unresolved: Vec::new(),
        })
    }

    /// Wraps an atlas the runner already built (the startup bake), so the
    /// worker result is not thrown away.
    #[must_use]
    pub fn from_font(bytes: Vec<u8>, font: Arc<Font>) -> Self {
        Self {
            bytes,
            chars: charset::latin_western(),
            font,
            bakes: 0,
            unresolved: Vec::new(),
        }
    }

    /// The current atlas.
    #[must_use]
    pub fn font(&self) -> &Arc<Font> {
        &self.font
    }

    /// How many times the atlas was re-baked for fallback glyphs.
    #[must_use]
    pub fn bakes(&self) -> u32 {
        self.bakes
    }

    /// The characters baked into the current atlas.
    #[must_use]
    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    /// Whether `ch` is already drawable from the current atlas.
    #[must_use]
    pub fn has(&self, ch: char) -> bool {
        ch.is_whitespace()
            || self.font.atlas.metrics.contains_key(&ch)
            || self.unresolved.contains(&ch)
    }

    /// The requested characters the atlas is still missing.
    #[must_use]
    pub fn missing<'a>(&self, requested: impl IntoIterator<Item = &'a char>) -> Vec<char> {
        let mut missing: Vec<char> = Vec::new();
        for ch in requested {
            if !self.has(*ch) && !missing.contains(ch) {
                missing.push(*ch);
            }
        }
        missing
    }

    /// Re-bakes the atlas if `requested` contains a character it lacks.
    ///
    /// Returns `true` when a new atlas was built (the caller must re-bind it).
    /// ASCII-only requests return `false` without touching the OS fonts.
    pub fn ensure(
        &mut self,
        requested: &[char],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> bool {
        let missing = self.missing(requested.iter());
        if missing.is_empty() {
            return false;
        }
        let Ok(parser) = ferrous_font::FontParser::new(self.bytes.clone()) else {
            return false;
        };
        let fallback = system::find_for(&missing);
        let resolvable = |ch: char| {
            parser.get_glyph_index(ch) != 0
                || fallback.iter().any(|(_, covered)| covered.contains(&ch))
        };
        // Remember what no face covers, so it is never requested again.
        for ch in &missing {
            if !resolvable(*ch) && !self.unresolved.contains(ch) {
                self.unresolved.push(*ch);
            }
        }
        let mut chars = self.chars.clone();
        for ch in &missing {
            if resolvable(*ch) && !chars.contains(ch) {
                chars.push(*ch);
            }
        }
        if chars.len() == self.chars.len() {
            // Nothing new is drawable: keep the current atlas, no re-bake.
            return false;
        }
        let primary: Vec<char> = chars
            .iter()
            .copied()
            .filter(|ch| parser.get_glyph_index(*ch) != 0)
            .collect();
        let mut owned: Vec<(Vec<u8>, Vec<char>)> = vec![(self.bytes.clone(), primary)];
        owned.extend(fallback);
        let faces: Vec<(&[u8], Vec<char>)> = owned
            .iter()
            .map(|(bytes, covered)| (bytes.as_slice(), covered.clone()))
            .collect();
        let Ok(font) = Font::load_multi(&faces, device, queue) else {
            return false;
        };
        self.font = Arc::new(font);
        self.chars = chars;
        self.bakes += 1;
        true
    }

    /// The GPU bytes of the current atlas.
    #[must_use]
    pub fn atlas_bytes(&self) -> u64 {
        self.font.atlas.gpu_bytes()
    }
}
