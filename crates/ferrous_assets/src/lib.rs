//! Asset utilities for FerrousEngine

pub mod atlas;
pub mod binary_reader;
pub mod msdf_gen;
pub mod path;
pub mod tables;

pub mod font_parser {
    use crate::binary_reader::*;
    use crate::path::GlyphCommand;
    use crate::tables::TableRecord;
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
            parser.read_offset_and_directory().map_err(|e| e.to_string())?;
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
                self.tables.insert(tag, TableRecord { tag, checksum, offset, length });
            }
            Ok(())
        }

        fn read_head(&mut self) -> std::io::Result<()> {
            let rec = self.tables.get(b"head").ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no head"))?;
            let mut cur = std::io::Cursor::new(&self.data[rec.offset as usize..rec.offset as usize + rec.length as usize]);
            cur.set_position(18);
            self.units_per_em = read_u16_be(&mut cur)?;
            cur.set_position(50);
            self.index_to_loc_format = read_i16_be(&mut cur)?;
            Ok(())
        }

        fn read_loca(&self) -> std::io::Result<()> {
            if !self.tables.contains_key(b"loca") { return Err(std::io::Error::new(std::io::ErrorKind::Other, "no loca")); }
            Ok(())
        }

        pub fn get_glyph_index(&self, c: char) -> u16 {
            let rec = match self.tables.get(b"cmap") { Some(r) => r, None => return 0 };
            let sub = &self.data[rec.offset as usize..rec.offset as usize + rec.length as usize];
            let mut cur = std::io::Cursor::new(sub);
            
            let _ = read_u16_be(&mut cur).unwrap_or(0); // Version
            let num_subtables = read_u16_be(&mut cur).unwrap_or(0);
            
            for _ in 0..num_subtables {
                let _ = read_u16_be(&mut cur).unwrap_or(0); // Platform
                let _ = read_u16_be(&mut cur).unwrap_or(0); // Encoding
                let offset = read_u32_be(&mut cur).unwrap_or(0);
                
                let saved = cur.position();
                cur.set_position(offset as u64);
                let format = read_u16_be(&mut cur).unwrap_or(0);
                
                if format == 4 {
                    let _ = read_u16_be(&mut cur).unwrap_or(0); // Length
                    let _ = read_u16_be(&mut cur).unwrap_or(0); // Language
                    let seg_count = read_u16_be(&mut cur).unwrap_or(0) / 2;
                    let _ = read_u16_be(&mut cur).unwrap_or(0); // SearchRange
                    let _ = read_u16_be(&mut cur).unwrap_or(0); // EntrySelector
                    let _ = read_u16_be(&mut cur).unwrap_or(0); // RangeShift
                    
                    let mut end_codes = vec![0u16; seg_count as usize];
                    for e in &mut end_codes { *e = read_u16_be(&mut cur).unwrap_or(0); }
                    
                    let _ = read_u16_be(&mut cur).unwrap_or(0); // ReservedPad
                    
                    let mut start_codes = vec![0u16; seg_count as usize];
                    for s in &mut start_codes { *s = read_u16_be(&mut cur).unwrap_or(0); }
                    
                    let mut id_deltas = vec![0i16; seg_count as usize];
                    for d in &mut id_deltas { *d = read_i16_be(&mut cur).unwrap_or(0); }
                    
                    // ¡AQUÍ ESTÁ LA MAGIA! Guardamos el offset base EXACTO en memoria.
                    let id_offsets_base = cur.position();
                    let mut id_offsets = vec![0u16; seg_count as usize];
                    for r in &mut id_offsets { *r = read_u16_be(&mut cur).unwrap_or(0); }
                    
                    let code = c as u32;
                    for i in 0..seg_count as usize {
                        if code >= start_codes[i] as u32 && code <= end_codes[i] as u32 {
                            if id_offsets[i] == 0 {
                                return ((code as i32 + id_deltas[i] as i32) % 65536) as u16;
                            } else {
                                // Cálculo offset TTF correcto referenciado desde sí mismo
                                let offset_addr = id_offsets_base + (i as u64 * 2);
                                let pos = offset_addr + id_offsets[i] as u64 + ((code - start_codes[i] as u32) as u64 * 2);
                                if pos as usize + 2 <= sub.len() {
                                    let val = u16::from_be_bytes([sub[pos as usize], sub[pos as usize + 1]]);
                                    if val == 0 { return 0; }
                                    return ((val as i32 + id_deltas[i] as i32) % 65536) as u16;
                                }
                            }
                        }
                    }
                }
                cur.set_position(saved);
            }
            0
        }

        fn parse_glyph(&self, glyph_index: u16) -> Option<Vec<GlyphCommand>> {
            let loca_rec = self.tables.get(b"loca")?;
            let glyf_rec = self.tables.get(b"glyf")?;
            let mut cur_loca = std::io::Cursor::new(&self.data[loca_rec.offset as usize..]);
            
            // CORREGIDO: En tu código original faltaba hacer set_position para index_to_loc_format == 0
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
            
            if off0 == off1 { return Some(Vec::new()); }
            
            let mut cur = std::io::Cursor::new(&self.data[glyf_rec.offset as usize + off0 as usize..]);
            let number_of_contours = read_i16_be(&mut cur).ok()?;
            if number_of_contours < 0 { return None; } // Glyphs compuestos saltados por ahora

            let _ = cur.seek(std::io::SeekFrom::Current(8)); // Saltamos Bounding Box
            let mut end_pts = Vec::new();
            for _ in 0..number_of_contours { end_pts.push(read_u16_be(&mut cur).ok()?); }
            let inst_len = read_u16_be(&mut cur).ok()? as i64;
            let _ = cur.seek(std::io::SeekFrom::Current(inst_len));

            let total_points = end_pts.last().map(|&v| v as usize + 1).unwrap_or(0);
            if total_points == 0 { return Some(Vec::new()); }

            let mut flags = Vec::with_capacity(total_points);
            while flags.len() < total_points {
                let mut b = [0u8; 1]; cur.read_exact(&mut b).ok()?;
                let f = b[0];
                flags.push(f);
                if f & 0x08 != 0 {
                    cur.read_exact(&mut b).ok()?;
                    for _ in 0..b[0] { flags.push(f); }
                }
            }

            let mut xs = Vec::with_capacity(total_points);
            let mut cur_x = 0i32;
            for &f in &flags {
                let dx = if f & 0x02 != 0 {
                    let mut b =[0u8; 1]; cur.read_exact(&mut b).ok()?;
                    let v = b[0] as i32;
                    if f & 0x10 != 0 { v } else { -v }
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
                    let mut b =[0u8; 1]; cur.read_exact(&mut b).ok()?;
                    let v = b[0] as i32;
                    if f & 0x20 != 0 { v } else { -v }
                } else if f & 0x20 != 0 {
                    0
                } else {
                    read_i16_be(&mut cur).ok()? as i32
                };
                cur_y = cur_y.wrapping_add(dy);
                ys.push(cur_y);
            }

            struct RawPoint { x: i32, y: i32, on_curve: bool }
            let mut points = Vec::new();
            for i in 0..total_points {
                points.push(RawPoint { x: xs[i], y: ys[i], on_curve: flags[i] & 0x01 != 0 });
            }

            let normalize = |x: i32, y: i32| -> (f32, f32) {
                (x as f32 / self.units_per_em as f32, y as f32 / self.units_per_em as f32)
            };
            
            let mut commands = Vec::new();
            let mut start_idx = 0;
            
            for &end_pt in &end_pts {
                let end_idx = end_pt as usize;
                let contour = &points[start_idx..=end_idx];
                if contour.is_empty() { start_idx = end_idx + 1; continue; }

                let mut interp = Vec::new();
                for i in 0..contour.len() {
                    interp.push(RawPoint { x: contour[i].x, y: contour[i].y, on_curve: contour[i].on_curve });
                    let next = &contour[(i + 1) % contour.len()];
                    if !contour[i].on_curve && !next.on_curve {
                        interp.push(RawPoint {
                            x: (contour[i].x + next.x) / 2,
                            y: (contour[i].y + next.y) / 2,
                            on_curve: true
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
                            &interp[i+1]
                        } else {
                            &RawPoint {
                                x: (sx * self.units_per_em as f32).round() as i32,
                                y: (sy * self.units_per_em as f32).round() as i32,
                                on_curve: true
                            }
                        };
                        let (cx, cy) = normalize(p.x, p.y);
                        let (nx, ny) = normalize(next.x, next.y);
                        commands.push(GlyphCommand::QuadTo { ctrl_x: cx, ctrl_y: cy, to_x: nx, to_y: ny });
                        i += 2;
                    }
                }
                
                // Cerrar el contorno asegurando que terminamos donde empezamos
                let (lx, ly) = match commands.last().unwrap() {
                    GlyphCommand::MoveTo(x, y) => (*x, *y),
                    GlyphCommand::LineTo(x, y) => (*x, *y),
                    GlyphCommand::QuadTo { to_x, to_y, .. } => (*to_x, *to_y)
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
            self.parse_glyph(idx).unwrap_or_default()
        }

        pub fn get_glyph_advance(&self, c: char) -> f32 {
            let idx = self.get_glyph_index(c);
            let hmtx_rec = match self.tables.get(b"hmtx") { Some(r) => r, None => return 0.0 };
            let hhea_rec = match self.tables.get(b"hhea") { Some(r) => r, None => return 0.0 };
            let mut cur = std::io::Cursor::new(&self.data);
            cur.set_position(hhea_rec.offset as u64 + 34);
            let num_h = read_u16_be(&mut cur).unwrap_or(0);
            let mut adv = 0u16;
            if idx < num_h { cur.set_position(hmtx_rec.offset as u64 + (idx as u64 * 4)); adv = read_u16_be(&mut cur).unwrap_or(0); }
            else { cur.set_position(hmtx_rec.offset as u64 + ((num_h as u64 - 1) * 4)); adv = read_u16_be(&mut cur).unwrap_or(0); }
            adv as f32 / self.units_per_em as f32
        }
    }
}

pub use atlas::{FontAtlas, GlyphMetrics};
