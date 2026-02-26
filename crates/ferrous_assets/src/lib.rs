//! Asset utilities for FerrousEngine

pub mod binary_reader;

/// Structures and logic for parsing TrueType fonts.
pub mod font_parser {
    use crate::binary_reader::*;
    use std::collections::HashMap;
    use std::io::Read;

    #[derive(Debug, Clone)]
    pub struct TableRecord {
        pub tag: [u8; 4],
        pub checksum: u32,
        pub offset: u32,
        pub length: u32,
    }

    pub struct FontParser {
        data: Vec<u8>,
        tables: HashMap<[u8; 4], TableRecord>,
        index_to_loc_format: i16,
    }

    impl FontParser {
        /// Create a new `FontParser` from raw font bytes. It will read the
        /// offset table and directory immediately.
        pub fn new(data: Vec<u8>) -> Result<Self, String> {
            let mut parser = FontParser {
                data,
                tables: HashMap::new(),
                index_to_loc_format: 0,
            };
            parser
                .read_offset_and_directory()
                .map_err(|e| e.to_string())?;
            parser.read_head().map_err(|e| e.to_string())?;
            parser.read_loca().map_err(|e| e.to_string())?;
            Ok(parser)
        }

        fn read_offset_and_directory(&mut self) -> std::io::Result<()> {
            let mut cur = std::io::Cursor::new(&self.data);
            // Offset table: scaler type (4 bytes), numTables (u16), searchRange,u16,
            //   entrySelector,u16, rangeShift,u16
            let _scaler_type = read_u32_be(&mut cur)?;
            let num_tables = read_u16_be(&mut cur)?;
            let _search_range = read_u16_be(&mut cur)?;
            let _entry_selector = read_u16_be(&mut cur)?;
            let _range_shift = read_u16_be(&mut cur)?;

            for _ in 0..num_tables {
                let mut tag = [0u8; 4];
                cur.read_exact(&mut tag)?;
                let checksum = read_u32_be(&mut cur)?;
                let offset = read_u32_be(&mut cur)?;
                let length = read_u32_be(&mut cur)?;
                let rec = TableRecord {
                    tag,
                    checksum,
                    offset,
                    length,
                };
                self.tables.insert(tag, rec);
            }
            Ok(())
        }

        fn read_head(&mut self) -> std::io::Result<()> {
            let tag = *b"head";
            let rec = self
                .tables
                .get(&tag)
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no head table"))?;
            let start = rec.offset as usize;
            let mut cur = std::io::Cursor::new(&self.data[start..(start + rec.length as usize)]);
            // skip major/minor, fontRevision, checksumAdjustment, magicNumber
            cur.set_position(12);
            self.index_to_loc_format = read_i16_be(&mut cur)?;
            Ok(())
        }

        fn read_loca(&self) -> std::io::Result<()> {
            // just ensure the table exists for now
            let tag = *b"loca";
            if !self.tables.contains_key(&tag) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "no loca table",
                ));
            }
            Ok(())
        }

        pub fn get_glyph_index(&self, c: char) -> u16 {
            if let Some(rec) = self.tables.get(b"cmap") {
                let start = rec.offset as usize;
                let sub = &self.data[start..(start + rec.length as usize)];
                // search for format 4
                let mut cur = std::io::Cursor::new(sub);
                let _version = read_u16_be(&mut cur).unwrap_or(0);
                let num_subtables = read_u16_be(&mut cur).unwrap_or(0);
                for _ in 0..num_subtables {
                    let _platform_id = read_u16_be(&mut cur).unwrap_or(0);
                    let _encoding_id = read_u16_be(&mut cur).unwrap_or(0);
                    let offset = read_u32_be(&mut cur).unwrap_or(0);
                    let saved_pos = cur.position();
                    cur.set_position(offset as u64);
                    let format = read_u16_be(&mut cur).unwrap_or(0);
                    if format == 4 {
                        // parse format 4 table
                        let _length = read_u16_be(&mut cur).unwrap_or(0);
                        let _language = read_u16_be(&mut cur).unwrap_or(0);
                        let seg_count_x2 = read_u16_be(&mut cur).unwrap_or(0);
                        let seg_count = seg_count_x2 / 2;
                        let _search_range = read_u16_be(&mut cur).unwrap_or(0);
                        let _entry_selector = read_u16_be(&mut cur).unwrap_or(0);
                        let _range_shift = read_u16_be(&mut cur).unwrap_or(0);
                        // read arrays
                        let mut end_codes = vec![0u16; seg_count as usize];
                        for e in &mut end_codes {
                            *e = read_u16_be(&mut cur).unwrap_or(0);
                        }
                        let _reserved_pad = read_u16_be(&mut cur).unwrap_or(0);
                        let mut start_codes = vec![0u16; seg_count as usize];
                        for s in &mut start_codes {
                            *s = read_u16_be(&mut cur).unwrap_or(0);
                        }
                        let mut id_deltas = vec![0i16; seg_count as usize];
                        for d in &mut id_deltas {
                            *d = read_i16_be(&mut cur).unwrap_or(0);
                        }
                        let mut id_range_offsets = vec![0u16; seg_count as usize];
                        for r in &mut id_range_offsets {
                            *r = read_u16_be(&mut cur).unwrap_or(0);
                        }
                        let glyph_array_pos = cur.position();
                        let code = c as u32;
                        for i in 0..seg_count as usize {
                            let start = start_codes[i] as u32;
                            let end = end_codes[i] as u32;
                            if code >= start && code <= end {
                                if id_range_offsets[i] == 0 {
                                    let glyph =
                                        ((code as i32 + id_deltas[i] as i32) % 65536) as u16;
                                    return glyph;
                                } else {
                                    let offset_in_seg = (code - start) as u64;
                                    let pos = glyph_array_pos
                                        + (i as u64 * 2)
                                        + offset_in_seg * 2
                                        + (id_range_offsets[i] as u64);
                                    if pos as usize + 2 <= sub.len() {
                                        let val = u16::from_be_bytes([
                                            sub[pos as usize],
                                            sub[pos as usize + 1],
                                        ]);
                                        if val == 0 {
                                            return 0;
                                        }
                                        return ((val as i32 + id_deltas[i] as i32) % 65536) as u16;
                                    }
                                }
                            }
                        }
                    }
                    cur.set_position(saved_pos);
                }
            }
            0
        }

        /// For debugging purposes we can query glyf offset from loca
        pub fn glyph_offset(&self, glyph_index: u16) -> Option<u32> {
            let loca = self.tables.get(b"loca")?;
            let glyf = self.tables.get(b"glyf")?;
            let start = loca.offset as usize;
            let slice = &self.data[start..(start + loca.length as usize)];
            if self.index_to_loc_format == 0 {
                let mut cur = std::io::Cursor::new(slice);
                for _ in 0..glyph_index {
                    let _off = read_u16_be(&mut cur).ok()?;
                    // multiplied by 2
                }
                // reposition cursor to read specific entry
                cur.set_position((glyph_index as u64) * 2);
                let off = read_u16_be(&mut cur).ok()? as u32 * 2;
                Some(glyf.offset + off)
            } else {
                let mut cur = std::io::Cursor::new(slice);
                cur.set_position((glyph_index as u64) * 4);
                let off = read_u32_be(&mut cur).ok()?;
                Some(glyf.offset + off)
            }
        }
    }
}

// tests for the crate
#[cfg(test)]
mod tests {
    use super::binary_reader::*;
    use super::font_parser::*;

    #[test]
    fn test_big_endian_reader() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut cur = std::io::Cursor::new(&data);
        assert_eq!(read_u16_be(&mut cur).unwrap(), 0x1234);
        assert_eq!(read_u16_be(&mut cur).unwrap(), 0x5678);
        cur.set_position(0);
        assert_eq!(read_u32_be(&mut cur).unwrap(), 0x12345678);
    }

    /// Build a minimal font containing a cmap table mapping 'A' -> glyph 5,
    /// a head table with indexToLocFormat=0, and a tiny loca.
    fn build_minimal_font() -> Vec<u8> {
        let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

        // cmap table
        let mut cmap = Vec::new();
        cmap.extend(&0u16.to_be_bytes()); // version
        cmap.extend(&1u16.to_be_bytes()); // numSubtables
        let subtable_record_pos = cmap.len();
        cmap.extend(&3u16.to_be_bytes()); // platform
        cmap.extend(&1u16.to_be_bytes()); // encoding
        cmap.extend(&0u32.to_be_bytes()); // offset placeholder

        let fmt_start = cmap.len();
        cmap.extend(&4u16.to_be_bytes()); // format
        cmap.extend(&0u16.to_be_bytes()); // length placeholder
        cmap.extend(&0u16.to_be_bytes()); // language
        cmap.extend(&2u16.to_be_bytes()); // segCountX2
        cmap.extend(&0u16.to_be_bytes()); // searchRange
        cmap.extend(&0u16.to_be_bytes()); // entrySelector
        cmap.extend(&0u16.to_be_bytes()); // rangeShift
        cmap.extend(&('A' as u16).to_be_bytes()); // endCodes
        cmap.extend(&0u16.to_be_bytes()); // reservedPad
        cmap.extend(&('A' as u16).to_be_bytes()); // startCodes
                                                  // choose delta so that glyph = (code + delta) mod 65536 = 5 for code 'A' (65)
                                                  // delta = 5 - 65 = -60
        cmap.extend(&(-60i16).to_be_bytes()); // idDeltas
        cmap.extend(&0u16.to_be_bytes()); // idRangeOffsets

        let fmt_length = (cmap.len() - fmt_start) as u16;
        cmap[fmt_start + 2..fmt_start + 4].copy_from_slice(&fmt_length.to_be_bytes());
        let offset_val = fmt_start as u32;
        cmap[subtable_record_pos + 4..subtable_record_pos + 8]
            .copy_from_slice(&offset_val.to_be_bytes());

        tables.push((*b"cmap", cmap));

        // head table with indexToLocFormat = 0 at byte 50
        let mut head = vec![0u8; 54];
        head[50..52].copy_from_slice(&0i16.to_be_bytes());
        tables.push((*b"head", head));

        // loca table with two entries (0,2) for format0
        let mut loca = Vec::new();
        loca.extend(&0u16.to_be_bytes());
        loca.extend(&2u16.to_be_bytes());
        tables.push((*b"loca", loca));

        // assemble
        let mut data = Vec::new();
        data.extend(&0u32.to_be_bytes());
        let num_tables = tables.len() as u16;
        data.extend(&num_tables.to_be_bytes());
        data.extend(&0u16.to_be_bytes());
        data.extend(&0u16.to_be_bytes());
        data.extend(&0u16.to_be_bytes());

        let mut offset = 12 + (16 * tables.len());
        let mut positions = Vec::new();
        for (_, tbl) in &tables {
            positions.push(offset as u32);
            offset += tbl.len();
        }
        for ((tag, tbl), &pos) in tables.iter().zip(&positions) {
            data.extend(tag);
            data.extend(&0u32.to_be_bytes());
            data.extend(&pos.to_be_bytes());
            data.extend(&(tbl.len() as u32).to_be_bytes());
        }
        for (_, tbl) in &tables {
            data.extend(tbl);
        }
        data
    }

    #[test]
    fn test_font_parser_cmap() {
        let font = build_minimal_font();
        let parser = FontParser::new(font).expect("parser must succeed");
        assert_eq!(parser.get_glyph_index('A'), 5);
        assert_eq!(parser.get_glyph_index('B'), 0);
        // no glyf table in the minimal font, glyph_offset should return None
        assert!(parser.glyph_offset(0).is_none());
    }
}
