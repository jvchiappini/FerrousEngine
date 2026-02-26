//! Asset utilities for FerrousEngine


pub mod binary_reader;
pub mod tables;
pub mod path;
pub mod msdf_gen;
pub mod atlas;

/// Structures and logic for parsing TrueType fonts.
pub mod font_parser {
    use crate::binary_reader::*;
    use std::collections::HashMap;
    use std::io::{Read, Seek};
    use crate::tables::TableRecord;
    use crate::path::GlyphCommand;

    // using TableRecord from tables.rs
    // pub use was removed because the root re-exports it directly

    pub struct FontParser {
        data: Vec<u8>,
        tables: HashMap<[u8; 4], TableRecord>,
        index_to_loc_format: i16,
        /// value read from head table, used to normalize glyph coordinates
        units_per_em: u16,
    }

    /// A simplified representation of drawing commands for a glyph.  This is
    /// deliberately small – only what the engine needs for the Manim-style
    /// renderer we are building.  Coordinates are normalized (divided by
    /// `units_per_em`) and are in the same coordinate space as the original
    /// font (y increasing upward).
    // path module exposes GlyphCommand
    // same for GlyphCommand

    impl FontParser {
        /// Create a new `FontParser` from raw font bytes. It will read the
        /// offset table and directory immediately.
        pub fn new(data: Vec<u8>) -> Result<Self, String> {
            let mut parser = FontParser {
                data,
                tables: HashMap::new(),
                index_to_loc_format: 0,
                units_per_em: 0,
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
            // head structure (see OpenType spec):
            // 0: majorVersion u16
            // 2: minorVersion u16
            // 4: fontRevision Fixed (32bits)
            // 8: checkSumAdjustment u32
            // 12: magicNumber u32
            // 16: flags u16
            // 18: unitsPerEm u16
            // ...
            // 50: indexToLocFormat i16

            // read unitsPerEm at offset 18
            cur.set_position(18);
            self.units_per_em = read_u16_be(&mut cur)?;
            // read indexToLocFormat at offset 50
            cur.set_position(50);
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

        /// Testing helper: return the raw bytes of the cmap table (if present).
        #[cfg(test)]
        pub fn debug_cmap_bytes(&self) -> Option<&[u8]> {
            self.tables.get(b"cmap").map(|rec| {
                let start = rec.offset as usize;
                &self.data[start..start + rec.length as usize]
            })
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

        /// Return the offset (absolute in file) of the specified glyph.  This is
        /// a thin wrapper around `glyph_offset` which returns `None` if the glyph
        /// has zero length (empty) or the offset table is missing.
        fn get_glyph_data_offset(&self, glyph_index: u16) -> Option<usize> {
            self.glyph_offset(glyph_index).map(|o| o as usize)
        }

        /// Parse the glyph outline for the given glyph index.  Only simple glyphs
        /// (numberOfContours >= 0) are handled; composite glyphs return `None`.
        fn parse_glyph(&self, glyph_index: u16) -> Option<Vec<GlyphCommand>> {
            let glyf_rec = self.tables.get(b"glyf")?;
            let off = self.get_glyph_data_offset(glyph_index)?;
            // if offset points to end of table or beyond, nothing to parse
            if off as u32 >= glyf_rec.offset + glyf_rec.length {
                return None;
            }
            let relative = off - glyf_rec.offset as usize;
            let slice = &self.data
                [glyf_rec.offset as usize..(glyf_rec.offset as usize + glyf_rec.length as usize)];
            let mut cur = std::io::Cursor::new(&slice[relative..]);

            // header
            let number_of_contours = read_i16_be(&mut cur).ok()?;
            let _x_min = read_i16_be(&mut cur).ok()?;
            let _y_min = read_i16_be(&mut cur).ok()?;
            let _x_max = read_i16_be(&mut cur).ok()?;
            let _y_max = read_i16_be(&mut cur).ok()?;

            if number_of_contours < 0 {
                // composite glyph: skip for now
                return None;
            }

            let contour_count = number_of_contours as usize;
            // read end points
            let mut end_pts = Vec::with_capacity(contour_count);
            for _ in 0..contour_count {
                end_pts.push(read_u16_be(&mut cur).ok()?);
            }
            let instruction_length = read_u16_be(&mut cur).ok()? as usize;
            // skip instructions
            let _ = cur.seek(std::io::SeekFrom::Current(instruction_length as i64));

            // total points = last end point + 1, or 0 if none
            let total_points = end_pts.last().map(|v| *v as usize + 1).unwrap_or(0);
            if total_points == 0 {
                return Some(Vec::new());
            }

            // read flags with repeat logic
            #[derive(Clone, Copy)]
            struct RawPoint {
                x: i32,
                y: i32,
                on_curve: bool,
            }

            let mut flags: Vec<u8> = Vec::with_capacity(total_points);
            while flags.len() < total_points {
                let flag = {
                    let mut buf = [0u8; 1];
                    cur.read_exact(&mut buf).ok()?;
                    buf[0]
                };
                flags.push(flag);
                if flag & 0x08 != 0 {
                    // repeat
                    let mut buf = [0u8; 1];
                    cur.read_exact(&mut buf).ok()?;
                    let count = buf[0] as usize;
                    for _ in 0..count {
                        flags.push(flag);
                    }
                }
            }

            // read coordinate deltas
            let mut points: Vec<RawPoint> = Vec::with_capacity(total_points);
            let mut cur_x = 0i32;
            let mut cur_y = 0i32;
            for &flag in &flags {
                // x
                let dx = if flag & 0x02 != 0 {
                    // x-short vector
                    let mut buf = [0u8; 1];
                    cur.read_exact(&mut buf).ok()?;
                    let val = buf[0] as i32;
                    if flag & 0x10 != 0 {
                        val
                    } else {
                        -val
                    }
                } else if flag & 0x10 != 0 {
                    0
                } else {
                    read_i16_be(&mut cur).ok()? as i32
                };
                cur_x = cur_x.wrapping_add(dx);

                // y
                let dy = if flag & 0x04 != 0 {
                    let mut buf = [0u8; 1];
                    cur.read_exact(&mut buf).ok()?;
                    let val = buf[0] as i32;
                    if flag & 0x20 != 0 {
                        val
                    } else {
                        -val
                    }
                } else if flag & 0x20 != 0 {
                    0
                } else {
                    read_i16_be(&mut cur).ok()? as i32
                };
                cur_y = cur_y.wrapping_add(dy);

                points.push(RawPoint {
                    x: cur_x,
                    y: cur_y,
                    on_curve: flag & 0x01 != 0,
                });
            }

            // helper to normalize a RawPoint to normalized f32
            let normalize = |x: i32, y: i32| -> (f32, f32) {
                let scale = self.units_per_em as f32;
                (x as f32 / scale, y as f32 / scale)
            };

            // build commands contour by contour
            let mut commands: Vec<GlyphCommand> = Vec::new();
            let mut start_index = 0;
            for &end_pt in &end_pts {
                let end_index = end_pt as usize;
                if end_index < start_index || end_index >= points.len() {
                    break; // malformed
                }
                let contour = &points[start_index..=end_index];
                if contour.is_empty() {
                    start_index = end_index + 1;
                    continue;
                }

                // insert implied on-curve points between consecutive off-curves
                let mut interp: Vec<RawPoint> = Vec::with_capacity(contour.len() * 2);
                for i in 0..contour.len() {
                    interp.push(contour[i]);
                    let next = &contour[(i + 1) % contour.len()];
                    if !contour[i].on_curve && !next.on_curve {
                        let mid_x = (contour[i].x + next.x) / 2;
                        let mid_y = (contour[i].y + next.y) / 2;
                        interp.push(RawPoint {
                            x: mid_x,
                            y: mid_y,
                            on_curve: true,
                        });
                    }
                }

                // ensure first point is on-curve
                if !interp[0].on_curve {
                    // compute midpoint between last and first
                    let last = interp.last().unwrap();
                    let mid_x = (last.x + interp[0].x) / 2;
                    let mid_y = (last.y + interp[0].y) / 2;
                    commands.push(GlyphCommand::MoveTo(
                        mid_x as f32 / self.units_per_em as f32,
                        mid_y as f32 / self.units_per_em as f32,
                    ));
                } else {
                    let (nx, ny) = normalize(interp[0].x, interp[0].y);
                    commands.push(GlyphCommand::MoveTo(nx, ny));
                }

                // iterate through interp points and generate line/quad commands
                let mut i = 0;
                while i < interp.len() {
                    if interp[i].on_curve {
                        let (_cx, _cy) = normalize(interp[i].x, interp[i].y);
                        if i + 1 < interp.len() {
                            if interp[i + 1].on_curve {
                                let (nx, ny) = normalize(interp[i + 1].x, interp[i + 1].y);
                                commands.push(GlyphCommand::LineTo(nx, ny));
                                i += 1;
                            } else if i + 2 < interp.len() && interp[i + 2].on_curve {
                                // quad with control at i+1
                                let (ctrlx, ctrly) = normalize(interp[i + 1].x, interp[i + 1].y);
                                let (nx, ny) = normalize(interp[i + 2].x, interp[i + 2].y);
                                commands.push(GlyphCommand::QuadTo {
                                    ctrl_x: ctrlx,
                                    ctrl_y: ctrly,
                                    to_x: nx,
                                    to_y: ny,
                                });
                                i += 2;
                            } else {
                                // shouldn't happen, fall back to line
                                let (nx, ny) = normalize(interp[i + 1].x, interp[i + 1].y);
                                commands.push(GlyphCommand::LineTo(nx, ny));
                                i += 1;
                            }
                        } else {
                            // last point; if it's not the starting point, close contour
                            // by drawing a line back to the first MoveTo coordinate
                            if let GlyphCommand::MoveTo(sx, sy) = commands.first().cloned().unwrap()
                            {
                                let (lx, ly) = normalize(interp[i].x, interp[i].y);
                                if (lx - sx).abs() > std::f32::EPSILON
                                    || (ly - sy).abs() > std::f32::EPSILON
                                {
                                    commands.push(GlyphCommand::LineTo(sx, sy));
                                }
                            }
                            i += 1;
                        }
                    } else {
                        // off-curve shouldn't appear because we inserted middles; advance
                        i += 1;
                    }
                }

                start_index = end_index + 1;
            }

            Some(commands)
        }

        /// Public API: given a character return its outline commands normalized by
        /// units per em.  If anything goes wrong or the glyph is empty we return an
        /// empty vector.  This is what the renderer will consume.
        pub fn get_glyph_outline(&self, c: char) -> Vec<GlyphCommand> {
            let idx = self.get_glyph_index(c);
            // even if idx == 0 we attempt to parse; some fonts may place a
            // valid outline at glyph 0 (notdef). Returning an empty vector only
            // if parsing fails or glyph data is absent.
            self.parse_glyph(idx).unwrap_or_default()
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
        // set a sane unitsPerEm value at offset 18 so division never panics
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
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
        // debug info to help diagnose mapping failure
        if let Some(bytes) = parser.debug_cmap_bytes() {
            eprintln!("cmap bytes: {:?}", bytes);
        }
        assert_eq!(parser.get_glyph_index('A'), 5);
        assert_eq!(parser.get_glyph_index('B'), 0);
        // no glyf table in the minimal font, glyph_offset should return None
        assert!(parser.glyph_offset(0).is_none());
    }

    /// Build a tiny font with a single simple glyph (a square) and map 'A' to
    /// it.  The glyph index used will be 0 and we use indexToLocFormat=1 to
    /// make building the loca table easier.
    fn build_font_with_simple_glyph() -> Vec<u8> {
        let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

        // cmap similar to minimal but map 'A' -> 0
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
        cmap.extend(&(-65i16).to_be_bytes()); // idDeltas: -65 to map 65->0
        cmap.extend(&0u16.to_be_bytes()); // idRangeOffsets

        let fmt_length = (cmap.len() - fmt_start) as u16;
        cmap[fmt_start + 2..fmt_start + 4].copy_from_slice(&fmt_length.to_be_bytes());
        let offset_val = fmt_start as u32;
        cmap[subtable_record_pos + 4..subtable_record_pos + 8]
            .copy_from_slice(&offset_val.to_be_bytes());

        tables.push((*b"cmap", cmap));

        // head table: indexToLocFormat = 1, unitsPerEm = 1000
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[50..52].copy_from_slice(&1i16.to_be_bytes());
        tables.push((*b"head", head));

        // glyf: simple square with 4 points
        let mut glyf = Vec::new();
        glyf.extend(&1i16.to_be_bytes()); // numberOfContours
        glyf.extend(&0i16.to_be_bytes()); // xMin
        glyf.extend(&0i16.to_be_bytes()); // yMin
        glyf.extend(&100i16.to_be_bytes()); // xMax
        glyf.extend(&100i16.to_be_bytes()); // yMax
        glyf.extend(&3u16.to_be_bytes()); // endPtsOfContours[0] = 3
        glyf.extend(&0u16.to_be_bytes()); // instructionLength
                                          // flags: four on-curve points, no shorts
        for _ in 0..4 {
            glyf.push(0x01);
        }
        // coords: deltas 0,0 ; 0,100 ;100,0 ;0,-100
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&100i16.to_be_bytes());
        glyf.extend(&100i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&(-100i16).to_be_bytes());
        tables.push((*b"glyf", glyf));

        // loca table with two entries (start of glyph0 and end)
        let mut loca = Vec::new();
        loca.extend(&0u32.to_be_bytes());
        let glyf_len = tables.iter().find(|(t, _)| t == b"glyf").unwrap().1.len() as u32;
        loca.extend(&glyf_len.to_be_bytes());
        tables.push((*b"loca", loca));

        // assemble everything same as earlier
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
    fn test_simple_glyph_outline() {
        let font = build_font_with_simple_glyph();
        let parser = FontParser::new(font).expect("parser must succeed");
        let outline = parser.get_glyph_outline('A');
        // must start with MoveTo and have at least one closing LineTo
        assert!(matches!(outline.first(), Some(GlyphCommand::MoveTo(_, _))));
        assert!(outline.len() >= 2);
        if let Some(GlyphCommand::LineTo(x, y)) = outline.last() {
            // last line should go back to origin (0,0) because our square
            assert!((x - 0.0).abs() < 1e-6 && (y - 0.0).abs() < 1e-6);
        } else {
            panic!("outline did not close with a LineTo");
        }
    }
}

// atlas types are defined in atlas.rs and re-exported below

// re-export atlas types to keep public API unchanged
pub use atlas::{GlyphMetrics, FontAtlas};

// additional tests for atlas and msdf generation
#[cfg(test)]
mod atlas_tests {
    use super::*;
    use wgpu::{Instance, InstanceDescriptor};
    // bring pollster into scope for async helpers
    use pollster;

    // duplicate simple glyph font builder so tests do not depend on private helper
    fn build_font_with_simple_glyph() -> Vec<u8> {
        let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        // cmap map 'A'->0
        let mut cmap = Vec::new();
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&1u16.to_be_bytes());
        let subtable_record_pos = cmap.len();
        cmap.extend(&3u16.to_be_bytes());
        cmap.extend(&1u16.to_be_bytes());
        cmap.extend(&0u32.to_be_bytes());
        let fmt_start = cmap.len();
        cmap.extend(&4u16.to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&2u16.to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&('A' as u16).to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        cmap.extend(&('A' as u16).to_be_bytes());
        cmap.extend(&(-65i16).to_be_bytes());
        cmap.extend(&0u16.to_be_bytes());
        let fmt_length = (cmap.len() - fmt_start) as u16;
        cmap[fmt_start + 2..fmt_start + 4].copy_from_slice(&fmt_length.to_be_bytes());
        let offset_val = fmt_start as u32;
        cmap[subtable_record_pos + 4..subtable_record_pos + 8]
            .copy_from_slice(&offset_val.to_be_bytes());
        tables.push((*b"cmap", cmap));
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[50..52].copy_from_slice(&1i16.to_be_bytes());
        tables.push((*b"head", head));
        let mut glyf = Vec::new();
        glyf.extend(&1i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&100i16.to_be_bytes());
        glyf.extend(&100i16.to_be_bytes());
        glyf.extend(&3u16.to_be_bytes());
        glyf.extend(&0u16.to_be_bytes());
        for _ in 0..4 { glyf.push(0x01); }
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&100i16.to_be_bytes());
        glyf.extend(&100i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&0i16.to_be_bytes());
        glyf.extend(&(-100i16).to_be_bytes());
        tables.push((*b"glyf", glyf));
        let mut loca = Vec::new();
        loca.extend(&0u32.to_be_bytes());
        let glyf_len = tables.iter().find(|(t, _)| t == b"glyf").unwrap().1.len() as u32;
        loca.extend(&glyf_len.to_be_bytes());
        tables.push((*b"loca", loca));
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
    fn simple_msdf_length() {
        let cmds = vec![font_parser::GlyphCommand::MoveTo(0.0, 0.0), font_parser::GlyphCommand::LineTo(1.0, 0.0)];
        // import generator locally
        use crate::msdf_gen::generate_msdf;
        let bmp = generate_msdf(&cmds, 8);
        assert_eq!(bmp.len(), 8 * 8 * 4);
    }

    #[test]
    #[ignore]
    fn build_atlas() {
        let instance = Instance::new(InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).unwrap();
        let font = build_font_with_simple_glyph();
        let parser = font_parser::FontParser::new(font).unwrap();
        let atlas = FontAtlas::new(&device, &queue, &parser, vec!['A']).unwrap();
        assert!(atlas.metrics.contains_key(&'A'));
    }
}
