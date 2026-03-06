//! High‑level font handling: parsing TrueType/OTF files, generating MSDF glyph
//! bitmaps and packing them into a GPU texture atlas.
//!
//! This crate powers text rendering across the engine but is optional; code that
//! does not need fonts can depend on `ferrous_assets --no-default-features` and
//! `ferrous_gui --no-default-features` to avoid pulling in the GPU/font logic.

pub mod atlas;
pub mod binary_reader;
pub mod parser;
pub mod path;
pub mod tables;
pub mod msdf_gen;

pub use atlas::{FontAtlas, GlyphMetrics};

/// High‑level font object holding an atlas; previously in `ferrous_assets`.
pub struct Font {
    pub atlas: FontAtlas,
}

impl Font {
    /// Load a font from the filesystem (desktop) or use a fallback (wasm).
    pub fn load(
        path: &str,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        chars: impl IntoIterator<Item = char>,
    ) -> Self {
        let char_list: Vec<char> = chars.into_iter().collect();

        // ── Desktop: read from filesystem ─────────────────────────────
        #[cfg(not(target_arch = "wasm32"))]
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "Warning: Font not found at {}: {}. Using fallback.",
                    path, e
                );
                Self::build_fallback_font()
            }
        };

        // ── wasm32: no filesystem, always use fallback
        #[cfg(target_arch = "wasm32")]
        let bytes = {
            let _ = path; // avoid unused variable warning
            Self::build_fallback_font()
        };

        Self::from_bytes_and_chars(bytes, char_list, device, queue)
    }

    /// Load a font from raw bytes in memory.  Useful for wasm builds or
    /// embedded assets.
    pub fn load_bytes(
        bytes: &[u8],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        chars: impl IntoIterator<Item = char>,
    ) -> Self {
        let char_list: Vec<char> = chars.into_iter().collect();
        Self::from_bytes_and_chars(bytes.to_vec(), char_list, device, queue)
    }

    fn from_bytes_and_chars(
        bytes: Vec<u8>,
        char_list: Vec<char>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        let parser = parser::FontParser::new(bytes).expect("Failed to parse font data");

        let final_chars = if char_list.is_empty() { vec!['A'] } else { char_list };

        let atlas = FontAtlas::new(device, queue, &parser, final_chars)
            .expect("Failed to build font atlas");

        Self { atlas }
    }

    /// Generate a minimal in-memory TrueType fallback font containing only
    /// the character 'A'.  Used when loading fails or on wasm targets.
    fn build_fallback_font() -> Vec<u8> {
        // (copy of the implementation previously in ferrous_assets)
        let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

        // cmap: 'A' -> ID 0
        let mut cmap = vec![0u16.to_be_bytes()[0], 0u16.to_be_bytes()[1]]; // version
        cmap.extend(&1u16.to_be_bytes()); // numSubtables
        let subtable_pos = cmap.len();
        cmap.extend(&3u16.to_be_bytes()); // platform
        cmap.extend(&1u16.to_be_bytes()); // encoding
        cmap.extend(&0u32.to_be_bytes()); // offset

        let fmt_start = cmap.len();
        cmap.extend(&4u16.to_be_bytes()); // format 4
        cmap.extend(&0u16.to_be_bytes()); // length
        cmap.extend(&0u16.to_be_bytes()); // lang
        cmap.extend(&2u16.to_be_bytes()); // segCountX2
        cmap.extend(&0u16.to_be_bytes()); // searchRange
        cmap.extend(&0u16.to_be_bytes()); // entrySelector
        cmap.extend(&0u16.to_be_bytes()); // rangeShift
        cmap.extend(&('A' as u16).to_be_bytes()); // endCodes
        cmap.extend(&0u16.to_be_bytes()); // pad
        cmap.extend(&('A' as u16).to_be_bytes()); // startCodes
        cmap.extend(&(-65i16).to_be_bytes()); // idDeltas
        cmap.extend(&0u16.to_be_bytes()); // idRangeOffsets
        let len = (cmap.len() - fmt_start) as u16;
        cmap[fmt_start + 2..fmt_start + 4].copy_from_slice(&len.to_be_bytes());
        let off = fmt_start as u32;
        cmap[subtable_pos + 4..subtable_pos + 8].copy_from_slice(&off.to_be_bytes());
        tables.push((*b"cmap", cmap));

        // head
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[50..52].copy_from_slice(&1i16.to_be_bytes());
        tables.push((*b"head", head));

        // glyf
        let mut glyf = Vec::new();
        glyf.extend(&1i16.to_be_bytes()); // 1 contour
        glyf.extend(&[0; 8]); // bbox
        glyf.extend(&3u16.to_be_bytes()); // endPt
        glyf.extend(&0u16.to_be_bytes()); // instLen
        for _ in 0..4 {
            glyf.push(0x01);
        }
        for v in &[0, 0, 0, 100, 100, 0, 0, -100] {
            glyf.extend(&((*v as i16).to_be_bytes()));
        }
        tables.push((*b"glyf", glyf));

        // loca
        let mut loca = Vec::new();
        loca.extend(&0u32.to_be_bytes());
        let glen = tables.iter().find(|(t, _)| t == b"glyf").unwrap().1.len() as u32;
        loca.extend(&glen.to_be_bytes());
        tables.push((*b"loca", loca));

        // hhea & hmtx (necessary for advance)
        let mut hhea = vec![0u8; 36];
        hhea[34..36].copy_from_slice(&1u16.to_be_bytes()); // numOfLongHorMetrics
        tables.push((*b"hhea", hhea));
        let mut hmtx = Vec::new();
        hmtx.extend(&1000u16.to_be_bytes()); // advance
        hmtx.extend(&0u16.to_be_bytes()); // lsb
        tables.push((*b"hmtx", hmtx));

        let mut data = vec![0, 0, 1, 0]; // scaler type
        data.extend(&(tables.len() as u16).to_be_bytes());
        data.extend(&[0; 6]); // search info

        let mut offset = 12 + (tables.len() * 16);
        for (tag, tbl) in &tables {
            data.extend(tag);
            data.extend(&0u32.to_be_bytes()); // checksum
            data.extend(&(offset as u32).to_be_bytes());
            data.extend(&(tbl.len() as u32).to_be_bytes());
            offset += tbl.len();
        }
        for (_, tbl) in &tables {
            data.extend(tbl);
        }
        data
    }
}
