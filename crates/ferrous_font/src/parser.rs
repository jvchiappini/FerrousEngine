use super::binary_reader::*;
use super::path::GlyphCommand;
use super::tables::TableRecord;
use std::collections::HashMap;
use std::io::{Read, Seek};

pub struct FontParser {
    data: Vec<u8>,
    tables: HashMap<[u8; 4], TableRecord>,
    index_to_loc_format: i16,
    pub units_per_em: u16,
}

impl FontParser {
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
        let _scaler_type = read_u32_be(&mut cur)?;
        let num_tables = read_u16_be(&mut cur)?;
        let _ = read_u16_be(&mut cur)?;
        let _ = read_u16_be(&mut cur)?;
        let _ = read_u16_be(&mut cur)?;
        for _ in 0..num_tables {
            let mut tag = [0u8; 4];
            cur.read_exact(&mut tag)?;
            let checksum = read_u32_be(&mut cur)?;
            let offset = read_u32_be(&mut cur)?;
            let length = read_u32_be(&mut cur)?;
            self.tables.insert(
                tag,
                TableRecord {
                    tag,
                    checksum,
                    offset,
                    length,
                },
            );
        }
        Ok(())
    }

    fn read_head(&mut self) -> std::io::Result<()> {
        let rec = self
            .tables
            .get(b"head")
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no head"))?;
        let mut cur = std::io::Cursor::new(
            &self.data[rec.offset as usize..rec.offset as usize + rec.length as usize],
        );
        cur.set_position(18);
        self.units_per_em = read_u16_be(&mut cur)?;
        cur.set_position(50);
        self.index_to_loc_format = read_i16_be(&mut cur)?;
        Ok(())
    }

    fn read_loca(&self) -> std::io::Result<()> {
        if !self.tables.contains_key(b"loca") {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "no loca"));
        }
        Ok(())
    }

    pub fn get_glyph_index(&self, c: char) -> u16 {
        let rec = match self.tables.get(b"cmap") {
            Some(r) => r,
            None => return 0,
        };
        let sub = &self.data[rec.offset as usize..rec.offset as usize + rec.length as usize];
        let mut cur = std::io::Cursor::new(sub);

        let _ = read_u16_be(&mut cur).unwrap_or(0); // version
        let num_subtables = read_u16_be(&mut cur).unwrap_or(0);

        // Collect all (platform, encoding, offset) entries first so we can
        // prioritise the best subtable:
        //   priority 1 — platform 3 encoding 10 (Windows full Unicode / format 12)
        //   priority 2 — platform 0 encoding 4  (Unicode 2.0+ / format 12)
        //   priority 3 — platform 3 encoding  1 (Windows BMP / format 4)
        //   priority 4 — platform 0 encoding  3 (Unicode BMP / format 4)
        //   fallback   — first parseable format 4 subtable
        struct SubEntry {
            platform: u16,
            encoding: u16,
            offset: u32,
        }
        let mut entries: Vec<SubEntry> = Vec::with_capacity(num_subtables as usize);
        for _ in 0..num_subtables {
            let platform = read_u16_be(&mut cur).unwrap_or(0);
            let encoding = read_u16_be(&mut cur).unwrap_or(0);
            let offset = read_u32_be(&mut cur).unwrap_or(0);
            entries.push(SubEntry {
                platform,
                encoding,
                offset,
            });
        }

        // Score: lower = better (will be sorted ascending)
        let score = |e: &SubEntry| -> u8 {
            match (e.platform, e.encoding) {
                (3, 10) => 0, // Windows full Unicode
                (0, 4) => 1,  // Unicode 2.0+ full
                (3, 1) => 2,  // Windows BMP
                (0, 3) => 3,  // Unicode BMP
                (0, 0) => 4,  // Unicode 1.0
                _ => 5,
            }
        };
        entries.sort_by_key(|e| score(e));

        let code = c as u32;

        for entry in &entries {
            let offset = entry.offset as usize;
            if offset + 2 > sub.len() {
                continue;
            }

            let format = u16::from_be_bytes([sub[offset], sub[offset + 1]]);

            // ── cmap format 12: full Unicode (handles > U+FFFF) ──────────
            if format == 12 {
                // format12 header: format(u16) reserved(u16) length(u32) language(u32)
                // numGroups(u32)
                if offset + 16 > sub.len() {
                    continue;
                }
                let num_groups = u32::from_be_bytes([
                    sub[offset + 12],
                    sub[offset + 13],
                    sub[offset + 14],
                    sub[offset + 15],
                ]) as usize;
                let base = offset + 16;
                if base + num_groups * 12 > sub.len() {
                    continue;
                }

                for g in 0..num_groups {
                    let p = base + g * 12;
                    let start_cp = u32::from_be_bytes([sub[p], sub[p + 1], sub[p + 2], sub[p + 3]]);
                    let end_cp =
                        u32::from_be_bytes([sub[p + 4], sub[p + 5], sub[p + 6], sub[p + 7]]);
                    let start_id =
                        u32::from_be_bytes([sub[p + 8], sub[p + 9], sub[p + 10], sub[p + 11]]);
                    if code >= start_cp && code <= end_cp {
                        let gid = start_id + (code - start_cp);
                        return gid.min(u16::MAX as u32) as u16;
                    }
                }
                // Character not in any group — try next subtable
                continue;
            }

            // ── cmap format 4: BMP Unicode ───────────────────────────────
            if format == 4 {
                if offset + 14 > sub.len() {
                    continue;
                }
                // length(u16) language(u16) segCountX2(u16) …
                let seg_count =
                    (u16::from_be_bytes([sub[offset + 6], sub[offset + 7]]) / 2) as usize;
                // header is 14 bytes, then endCodes[segCount], reservedPad, startCodes, idDeltas,
                // idRangeOffsets (each segCount u16s)
                let needed = offset + 14 + seg_count * 8 + 2;
                if needed > sub.len() {
                    continue;
                }

                let end_base = offset + 14;
                let start_base = end_base + seg_count * 2 + 2; // +2 for reservedPad
                let delta_base = start_base + seg_count * 2;
                let range_base = delta_base + seg_count * 2;

                if range_base + seg_count * 2 > sub.len() {
                    continue;
                }

                for i in 0..seg_count {
                    let end_code =
                        u16::from_be_bytes([sub[end_base + i * 2], sub[end_base + i * 2 + 1]])
                            as u32;
                    let start_code =
                        u16::from_be_bytes([sub[start_base + i * 2], sub[start_base + i * 2 + 1]])
                            as u32;
                    if code < start_code || code > end_code {
                        continue;
                    }

                    let id_delta =
                        i16::from_be_bytes([sub[delta_base + i * 2], sub[delta_base + i * 2 + 1]]);
                    let id_offset =
                        u16::from_be_bytes([sub[range_base + i * 2], sub[range_base + i * 2 + 1]]);

                    if id_offset == 0 {
                        return ((code as i32 + id_delta as i32).rem_euclid(65536)) as u16;
                    } else {
                        // The offset is relative to the idRangeOffset *field itself*
                        let range_field_pos = range_base + i * 2;
                        let glyph_pos =
                            range_field_pos + id_offset as usize + (code - start_code) as usize * 2;
                        if glyph_pos + 2 > sub.len() {
                            continue;
                        }
                        let glyph_id = u16::from_be_bytes([sub[glyph_pos], sub[glyph_pos + 1]]);
                        if glyph_id == 0 {
                            return 0;
                        }
                        return ((glyph_id as i32 + id_delta as i32).rem_euclid(65536)) as u16;
                    }
                }
                // Not found in this format-4 subtable — try next
                continue;
            }
        }

        0 // No subtable could map this character
    }

    /// Resolve the glyf data offset for `glyph_index` via the loca table.
    /// Returns `None` if the index is out of range or the tables are missing.
    fn glyf_offset(&self, glyph_index: u16) -> Option<(u32, u32)> {
        let loca_rec = self.tables.get(b"loca")?;
        let glyf_rec = self.tables.get(b"glyf")?;
        let mut cur_loca = std::io::Cursor::new(&self.data[loca_rec.offset as usize..]);

        let off0 = if self.index_to_loc_format == 0 {
            cur_loca.set_position(glyph_index as u64 * 2);
            read_u16_be(&mut cur_loca).ok()? as u32 * 2
        } else {
            cur_loca.set_position(glyph_index as u64 * 4);
            read_u32_be(&mut cur_loca).ok()?
        };
        let off1 = if self.index_to_loc_format == 0 {
            cur_loca.set_position(glyph_index as u64 * 2 + 2);
            read_u16_be(&mut cur_loca).ok()? as u32 * 2
        } else {
            cur_loca.set_position(glyph_index as u64 * 4 + 4);
            read_u32_be(&mut cur_loca).ok()?
        };

        let glyf_base = glyf_rec.offset;
        Some((glyf_base + off0, glyf_base + off1))
    }

    /// Parse a simple (non-composite) glyph.  Coordinates are pre-normalised
    /// by `units_per_em` and returned as [`GlyphCommand`]s.
    /// Returns `None` only on a hard parse error; an empty outline is `Some(vec![])`.
    fn parse_simple_glyph(&self, glyph_index: u16) -> Option<Vec<GlyphCommand>> {
        let (abs_off0, abs_off1) = self.glyf_offset(glyph_index)?;
        if abs_off0 == abs_off1 {
            return Some(Vec::new()); // empty glyph (e.g. space)
        }

        let glyf_data = &self.data[abs_off0 as usize..abs_off1 as usize];
        let mut cur = std::io::Cursor::new(glyf_data);
        let number_of_contours = read_i16_be(&mut cur).ok()?;
        // Composite glyphs are handled by `parse_composite_glyph`; bail here.
        if number_of_contours < 0 {
            return None;
        }

        Some(self.decode_simple_contours(&mut cur, number_of_contours as u16)?)
    }

    /// Walk a composite glyph (numberOfContours < 0) and accumulate the
    /// transformed outlines of every component glyph.  Handles nested
    /// composites recursively (with a depth-limit to prevent infinite loops).
    fn parse_composite_glyph(&self, glyph_index: u16, depth: u8) -> Option<Vec<GlyphCommand>> {
        if depth > 8 {
            return Some(Vec::new()); // safety: avoid infinite recursion
        }

        let (abs_off0, abs_off1) = self.glyf_offset(glyph_index)?;
        if abs_off0 == abs_off1 {
            return Some(Vec::new());
        }

        let glyf_data = &self.data[abs_off0 as usize..abs_off1 as usize];
        let mut cur = std::io::Cursor::new(glyf_data);
        let number_of_contours = read_i16_be(&mut cur).ok()?;
        if number_of_contours >= 0 {
            // Actually a simple glyph — delegate.
            return self.parse_simple_glyph(glyph_index);
        }

        // Skip bounding-box (4 × i16 = 8 bytes)
        cur.seek(std::io::SeekFrom::Current(8)).ok()?;

        // TTF composite component flags
        const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
        const ARGS_ARE_XY_VALUES: u16 = 0x0002;
        const WE_HAVE_A_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
        const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

        let mut all_commands: Vec<GlyphCommand> = Vec::new();

        loop {
            let flags = read_u16_be(&mut cur).ok()?;
            let comp_index = read_u16_be(&mut cur).ok()?;

            // Read dx/dy or point-index pair
            let (dx, dy) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                let a = read_i16_be(&mut cur).ok()? as f32;
                let b = read_i16_be(&mut cur).ok()? as f32;
                (a, b)
            } else {
                let a = cur.read_u8_or(0) as i8 as f32;
                let b = cur.read_u8_or(0) as i8 as f32;
                (a, b)
            };

            // Only ARGS_ARE_XY_VALUES gives us a translation; otherwise these
            // are point-pair indices which we can't resolve without the full
            // point set.  We fall back to zero offset in that case.
            let (tx, ty) = if flags & ARGS_ARE_XY_VALUES != 0 {
                (dx / self.units_per_em as f32, dy / self.units_per_em as f32)
            } else {
                (0.0, 0.0)
            };

            // Read optional transform (we only support uniform scale and 2×2)
            let (a, b, c, d) = if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                let a = read_f2dot14(&mut cur);
                let b = read_f2dot14(&mut cur);
                let c = read_f2dot14(&mut cur);
                let d = read_f2dot14(&mut cur);
                (a, b, c, d)
            } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                let sx = read_f2dot14(&mut cur);
                let sy = read_f2dot14(&mut cur);
                (sx, 0.0, 0.0, sy)
            } else if flags & WE_HAVE_A_SCALE != 0 {
                let s = read_f2dot14(&mut cur);
                (s, 0.0, 0.0, s)
            } else {
                (1.0_f32, 0.0_f32, 0.0_f32, 1.0_f32)
            };

            // Recursively get the component's outline
            let component_cmds = if depth < 8 {
                self.parse_glyph_recursive(comp_index, depth + 1)
            } else {
                Vec::new()
            };

            // Apply the 2×3 affine transform (a b c d tx ty) to every point
            let identity = a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0;
            let no_offset = tx == 0.0 && ty == 0.0;

            for cmd in component_cmds {
                let transformed = if identity && no_offset {
                    cmd
                } else {
                    transform_command(cmd, a, b, c, d, tx, ty)
                };
                all_commands.push(transformed);
            }

            if flags & MORE_COMPONENTS == 0 {
                break;
            }
        }

        Some(all_commands)
    }

    /// Shared entry-point: dispatches to simple or composite parser.
    fn parse_glyph_recursive(&self, glyph_index: u16, depth: u8) -> Vec<GlyphCommand> {
        let (abs_off0, abs_off1) = match self.glyf_offset(glyph_index) {
            Some(o) => o,
            None => return Vec::new(),
        };
        if abs_off0 == abs_off1 {
            return Vec::new(); // empty glyph (space etc.)
        }
        let glyf_data = &self.data[abs_off0 as usize..abs_off1 as usize];
        if glyf_data.len() < 2 {
            return Vec::new();
        }
        let number_of_contours = i16::from_be_bytes([glyf_data[0], glyf_data[1]]);

        if number_of_contours >= 0 {
            self.parse_simple_glyph(glyph_index).unwrap_or_default()
        } else {
            self.parse_composite_glyph(glyph_index, depth)
                .unwrap_or_default()
        }
    }

    /// Decode the point data for a simple (non-composite) glyph.
    /// `cur` must be positioned right after the `numberOfContours` i16.
    fn decode_simple_contours(
        &self,
        mut cur: &mut std::io::Cursor<&[u8]>,
        number_of_contours: u16,
    ) -> Option<Vec<GlyphCommand>> {
        // Skip bounding-box (xMin yMin xMax yMax — 4 × i16 = 8 bytes).
        // The cursor arrives here positioned right after the numberOfContours i16.
        let _ = cur.seek(std::io::SeekFrom::Current(8));
        let mut end_pts = Vec::new();
        for _ in 0..number_of_contours {
            end_pts.push(read_u16_be(&mut cur).ok()?);
        }
        let inst_len = read_u16_be(&mut cur).ok()? as i64;
        let _ = cur.seek(std::io::SeekFrom::Current(inst_len));

        let total_points = end_pts.last().map(|&v| v as usize + 1).unwrap_or(0);
        if total_points == 0 {
            return Some(Vec::new());
        }

        let mut flags = Vec::new();
        while flags.len() < total_points {
            let mut b = [0u8; 1];
            cur.read_exact(&mut b).ok()?;
            let f = b[0];
            flags.push(f);
            if f & 0x08 != 0 {
                cur.read_exact(&mut b).ok()?;
                for _ in 0..b[0] {
                    flags.push(f);
                }
            }
        }

        let mut xs = Vec::with_capacity(total_points);
        let mut cur_x = 0i32;
        for &f in &flags {
            let dx = if f & 0x02 != 0 {
                let mut b = [0u8; 1];
                cur.read_exact(&mut b).ok()?;
                let v = b[0] as i32;
                if f & 0x10 != 0 {
                    v
                } else {
                    -v
                }
            } else if f & 0x10 != 0 {
                0
            } else {
                read_i16_be(&mut cur).ok()? as i32
            };
            cur_x = cur_x.wrapping_add(dx);
            xs.push(cur_x);
        }

        let mut ys = Vec::with_capacity(total_points);
        let mut cur_y = 0i32;
        for &f in &flags {
            let dy = if f & 0x04 != 0 {
                let mut b = [0u8; 1];
                cur.read_exact(&mut b).ok()?;
                let v = b[0] as i32;
                if f & 0x20 != 0 {
                    v
                } else {
                    -v
                }
            } else if f & 0x20 != 0 {
                0
            } else {
                read_i16_be(&mut cur).ok()? as i32
            };
            cur_y = cur_y.wrapping_add(dy);
            ys.push(cur_y);
        }

        struct RawPoint {
            x: i32,
            y: i32,
            on_curve: bool,
        }
        let mut points = Vec::new();
        for i in 0..total_points {
            points.push(RawPoint {
                x: xs[i],
                y: ys[i],
                on_curve: flags[i] & 0x01 != 0,
            });
        }

        let normalize = |x: i32, y: i32| -> (f32, f32) {
            (
                x as f32 / self.units_per_em as f32,
                y as f32 / self.units_per_em as f32,
            )
        };

        let mut commands = Vec::new();
        let mut start_idx = 0;

        for &end_pt in &end_pts {
            let end_idx = end_pt as usize;
            let contour = &points[start_idx..=end_idx];
            if contour.is_empty() {
                start_idx = end_idx + 1;
                continue;
            }

            let mut interp = Vec::new();
            for i in 0..contour.len() {
                interp.push(RawPoint {
                    x: contour[i].x,
                    y: contour[i].y,
                    on_curve: contour[i].on_curve,
                });
                let next = &contour[(i + 1) % contour.len()];
                if !contour[i].on_curve && !next.on_curve {
                    interp.push(RawPoint {
                        x: (contour[i].x + next.x) / 2,
                        y: (contour[i].y + next.y) / 2,
                        on_curve: true,
                    });
                }
            }

            // CORRECCIÓN MATEMÁTICA TTF: Si la curva empieza off-curve, debemos inferir
            // matemáticamente el punto real de inicio promediando con el final (si también es off-curve)
            let (sx, sy) = if interp[0].on_curve {
                normalize(interp[0].x, interp[0].y)
            } else {
                let last = interp.last().unwrap();
                if last.on_curve {
                    normalize(last.x, last.y)
                } else {
                    normalize((last.x + interp[0].x) / 2, (last.y + interp[0].y) / 2)
                }
            };

            commands.push(GlyphCommand::MoveTo(sx, sy));

            let mut i = if interp[0].on_curve { 1 } else { 0 };
            while i < interp.len() {
                let p = &interp[i];
                if p.on_curve {
                    let (nx, ny) = normalize(p.x, p.y);
                    commands.push(GlyphCommand::LineTo(nx, ny));
                    i += 1;
                } else {
                    let next = if i + 1 < interp.len() {
                        &interp[i + 1]
                    } else {
                        &RawPoint {
                            x: (sx * self.units_per_em as f32).round() as i32,
                            y: (sy * self.units_per_em as f32).round() as i32,
                            on_curve: true,
                        }
                    };
                    let (cx, cy) = normalize(p.x, p.y);
                    let (nx, ny) = normalize(next.x, next.y);
                    commands.push(GlyphCommand::QuadTo {
                        ctrl_x: cx,
                        ctrl_y: cy,
                        to_x: nx,
                        to_y: ny,
                    });
                    i += 2;
                }
            }

            // Cerrar el contorno asegurando que terminamos donde empezamos
            let (lx, ly) = match commands.last().unwrap() {
                GlyphCommand::MoveTo(x, y) => (*x, *y),
                GlyphCommand::LineTo(x, y) => (*x, *y),
                GlyphCommand::QuadTo { to_x, to_y, .. } => (*to_x, *to_y),
            };
            if (lx - sx).abs() > 1e-6 || (ly - sy).abs() > 1e-6 {
                commands.push(GlyphCommand::LineTo(sx, sy));
            }

            start_idx = end_idx + 1;
        }
        Some(commands)
    }

    pub fn get_glyph_outline(&self, c: char) -> Vec<GlyphCommand> {
        let idx = self.get_glyph_index(c);
        self.parse_glyph_recursive(idx, 0)
    }

    pub fn get_glyph_advance(&self, c: char) -> f32 {
        let idx = self.get_glyph_index(c);
        let hmtx_rec = match self.tables.get(b"hmtx") {
            Some(r) => r,
            None => return 0.0,
        };
        let hhea_rec = match self.tables.get(b"hhea") {
            Some(r) => r,
            None => return 0.0,
        };
        let mut cur = std::io::Cursor::new(&self.data);
        cur.set_position(hhea_rec.offset as u64 + 34);
        let num_h = read_u16_be(&mut cur).unwrap_or(0);
        let adv = if idx < num_h {
            cur.set_position(hmtx_rec.offset as u64 + (idx as u64 * 4));
            read_u16_be(&mut cur).unwrap_or(0)
        } else {
            cur.set_position(hmtx_rec.offset as u64 + ((num_h as u64 - 1) * 4));
            read_u16_be(&mut cur).unwrap_or(0)
        };
        adv as f32 / self.units_per_em as f32
    }
}

// ── Free helpers ──────────────────────────────────────────────────────────────

/// Read a 16-bit F2Dot14 fixed-point number (2 integer + 14 fractional bits).
/// Used in TrueType composite glyph transformation matrices.
fn read_f2dot14<R: std::io::Read>(reader: &mut R) -> f32 {
    let mut buf = [0u8; 2];
    if reader.read_exact(&mut buf).is_err() {
        return 1.0;
    }
    let raw = i16::from_be_bytes(buf);
    raw as f32 / 16384.0
}

/// Apply a 2×3 affine transform to a single [`GlyphCommand`].
/// The matrix is `[a b c d]` (column-major 2×2) plus translation `[tx, ty]`.
fn transform_command(
    cmd: GlyphCommand,
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
) -> GlyphCommand {
    let pt = |x: f32, y: f32| -> (f32, f32) { (a * x + c * y + tx, b * x + d * y + ty) };
    match cmd {
        GlyphCommand::MoveTo(x, y) => {
            let (nx, ny) = pt(x, y);
            GlyphCommand::MoveTo(nx, ny)
        }
        GlyphCommand::LineTo(x, y) => {
            let (nx, ny) = pt(x, y);
            GlyphCommand::LineTo(nx, ny)
        }
        GlyphCommand::QuadTo {
            ctrl_x,
            ctrl_y,
            to_x,
            to_y,
        } => {
            let (cx, cy) = pt(ctrl_x, ctrl_y);
            let (nx, ny) = pt(to_x, to_y);
            GlyphCommand::QuadTo {
                ctrl_x: cx,
                ctrl_y: cy,
                to_x: nx,
                to_y: ny,
            }
        }
    }
}

/// Extension trait so `Cursor` can read a single byte with a fallback.
trait ReadU8Ext {
    fn read_u8_or(&mut self, fallback: u8) -> u8;
}
impl<T: std::io::Read> ReadU8Ext for T {
    fn read_u8_or(&mut self, fallback: u8) -> u8 {
        let mut b = [0u8; 1];
        self.read_exact(&mut b).map(|_| b[0]).unwrap_or(fallback)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn load_roboto() -> Option<FontParser> {
        // Try GUIMaker assets first, then engine assets
        let paths = [
            "GUIMaker/assets/fonts/Roboto-Regular.ttf",
            "assets/fonts/Roboto-Regular.ttf",
        ];
        for p in &paths {
            if let Ok(bytes) = std::fs::read(p) {
                return FontParser::new(bytes).ok();
            }
        }
        None
    }

    #[test]
    fn cmap_ascii_chars() {
        let Some(parser) = load_roboto() else { return };
        for c in 'A'..='Z' {
            let idx = parser.get_glyph_index(c);
            assert!(idx > 0, "ASCII '{c}' mapped to glyph 0");
        }
    }

    #[test]
    fn cmap_latin1_chars() {
        let Some(parser) = load_roboto() else { return };
        let chars = ['ñ', 'Ñ', 'á', 'é', 'í', 'ó', 'ú', 'ü', '¿', '¡'];
        for c in chars {
            let idx = parser.get_glyph_index(c);
            println!("'{}' (U+{:04X}) -> glyph {}", c, c as u32, idx);
            assert!(
                idx > 0,
                "Latin-1 char '{c}' (U+{:04X}) mapped to glyph 0",
                c as u32
            );
        }
    }

    #[test]
    fn cmap_bullet_and_symbols() {
        let Some(parser) = load_roboto() else { return };
        let chars = [
            ('•', 0x2022u32),
            ('—', 0x2014),
            ('€', 0x20AC),
            ('©', 0x00A9),
            ('™', 0x2122),
        ];
        for (c, cp) in chars {
            let idx = parser.get_glyph_index(c);
            println!("'{}' (U+{:04X}) -> glyph {}", c, cp, idx);
        }
    }

    #[test]
    fn outline_not_empty_for_simple_glyph() {
        let Some(parser) = load_roboto() else { return };
        let cmds = parser.get_glyph_outline('A');
        assert!(!cmds.is_empty(), "outline for 'A' should not be empty");
    }

    #[test]
    fn outline_not_empty_for_composite_glyph() {
        let Some(parser) = load_roboto() else { return };
        let cmds = parser.get_glyph_outline('ñ');
        println!("ñ outline commands: {}", cmds.len());
        assert!(
            !cmds.is_empty(),
            "outline for 'ñ' should not be empty (composite glyph)"
        );
    }

    #[test]
    fn dump_cmap_subtables() {
        let Some(parser) = load_roboto() else { return };
        // Print all cmap subtables so we can see what's available
        let rec = parser.tables.get(b"cmap").expect("no cmap");
        let sub = &parser.data[rec.offset as usize..rec.offset as usize + rec.length as usize];
        let version = u16::from_be_bytes([sub[0], sub[1]]);
        let num = u16::from_be_bytes([sub[2], sub[3]]);
        println!("cmap version={version}, numSubtables={num}");
        for i in 0..num as usize {
            let base = 4 + i * 8;
            let platform = u16::from_be_bytes([sub[base], sub[base + 1]]);
            let encoding = u16::from_be_bytes([sub[base + 2], sub[base + 3]]);
            let offset =
                u32::from_be_bytes([sub[base + 4], sub[base + 5], sub[base + 6], sub[base + 7]]);
            let fmt_off = offset as usize;
            let format = if fmt_off + 1 < sub.len() {
                u16::from_be_bytes([sub[fmt_off], sub[fmt_off + 1]])
            } else {
                999
            };
            println!("  subtable {i}: platform={platform} encoding={encoding} offset={offset} format={format}");
        }
    }
}
