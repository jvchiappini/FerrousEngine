pub mod atlas;
pub mod binary_reader;
pub mod msdf_gen;
pub mod parser;
pub mod path;
pub mod tables;

pub use atlas::{FontAtlas, GlyphMetrics};
use parser::FontParser;
use wgpu::{Device, Queue};

/// Estructura de alto nivel que representa una fuente lista para usar.
pub struct Font {
    pub atlas: FontAtlas,
}

impl Font {
    /// Carga una fuente desde el disco o usa una de respaldo si falla.
    pub fn load(
        path: &str,
        device: &Device,
        queue: &Queue,
        chars: impl IntoIterator<Item = char>,
    ) -> Self {
        let char_list: Vec<char> = chars.into_iter().collect();
        
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Warning: Font not found at {}: {}. Using fallback.", path, e);
                Self::build_fallback_font()
            }
        };

        let parser = FontParser::new(bytes).expect("Failed to parse font data");
        
        // Si usamos la fuente de respaldo, solo tiene el glifo 'A'
        let final_chars = if char_list.is_empty() { vec!['A'] } else { char_list };

        let atlas = FontAtlas::new(device, queue, &parser, final_chars)
            .expect("Failed to build font atlas");

        Self { atlas }
    }

    /// Genera una fuente TrueType mínima en memoria ('A' únicamente).
    fn build_fallback_font() -> Vec<u8> {
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
        cmap[fmt_start+2..fmt_start+4].copy_from_slice(&len.to_be_bytes());
        let off = fmt_start as u32;
        cmap[subtable_pos+4..subtable_pos+8].copy_from_slice(&off.to_be_bytes());
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
        for _ in 0..4 { glyf.push(0x01); } // flags
        for v in &[0, 0, 0, 100, 100, 0, 0, -100] { glyf.extend(&((*v as i16).to_be_bytes())); }
        tables.push((*b"glyf", glyf));

        // loca
        let mut loca = Vec::new();
        loca.extend(&0u32.to_be_bytes());
        let glen = tables.iter().find(|(t,_)| t==b"glyf").unwrap().1.len() as u32;
        loca.extend(&glen.to_be_bytes());
        tables.push((*b"loca", loca));

        // hhea & hmtx (necesarios para el avance)
        let mut hhea = vec![0u8; 36];
        hhea[34..36].copy_from_slice(&1u16.to_be_bytes()); // numOfLongHorMetrics
        tables.push((*b"hhea", hhea));
        let mut hmtx = Vec::new();
        hmtx.extend(&1000u16.to_be_bytes()); // advance
        hmtx.extend(&0u16.to_be_bytes()); // lsb
        tables.push((*b"hmtx", hmtx));

        let mut data = vec![0,0,1,0]; // scaler type
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
        for (_, tbl) in &tables { data.extend(tbl); }
        data
    }
}