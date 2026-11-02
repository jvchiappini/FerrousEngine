use super::binary_reader::*;
use super::path::GlyphCommand;
use super::tables::TableRecord;
use std::collections::HashMap;
use std::io::{Read, Seek};

/// A rectangle in em units (1.0 = one em).  Y points up and the origin is the
/// glyph's baseline origin (the pen position).
///
/// Used both for the font's global bounding box ([`FontMetrics::bbox`]) and for
/// individual glyph ink bounds ([`FontParser::get_glyph_bounds`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlyphBounds {
    /// Left edge (`xMin`).
    pub left: f32,
    /// Bottom edge (`yMin`).
    pub bottom: f32,
    /// Right edge (`xMax`).
    pub right: f32,
    /// Top edge (`yMax`).
    pub top: f32,
}

/// Vertical metrics of a font, normalised to em units.
///
/// The values come from the `head` and `hhea` tables:
///
/// * `ascender` / `descender` / `line_gap` are `hhea` values divided by
///   `units_per_em`; `descender` is negative for a normal font.
/// * `bbox` is the global glyph bounding box from `head`, also in em units.
///
/// Multiply by the layout size in pixels to obtain pixel values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FontMetrics {
    /// Font design units per em (`head.unitsPerEm`).
    pub units_per_em: u16,
    /// Distance from the baseline to the ascender (`hhea.ascender`), in em.
    pub ascender: f32,
    /// Distance from the baseline to the descender (`hhea.descender`), in em.
    /// Negative for a normal font.
    pub descender: f32,
    /// Recommended extra space between lines (`hhea.lineGap`), in em.
    pub line_gap: f32,
    /// Global glyph bounding box from `head`, in em.
    pub bbox: GlyphBounds,
}

/// Why a font could not be parsed.
///
/// The distinction matters to callers: [`Self::Unsupported`] means the file is
/// a valid font in an outline format this parser cannot read (CFF outlines,
/// CFF2 variable fonts), while [`Self::Parse`] means the bytes are malformed or
/// a required table is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The bytes are not a readable TrueType/OpenType font.
    Parse(String),
    /// The font is valid but uses an outline format this parser does not read.
    Unsupported(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "{message}"),
            Self::Unsupported(message) => write!(f, "unsupported font: {message}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// A pair-adjustment value from one GPOS lookup, in em units.
///
/// The value is the horizontal advance adjustment applied to the **first**
/// glyph of the pair (`ValueRecord1.xAdvance`).  Other GPOS adjustments
/// (placement, vertical, device tables, mark attachment) are out of scope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GposPair {
    /// First glyph of the pair, the one whose advance changes.
    pub left: u16,
    /// Second glyph of the pair.
    pub right: u16,
    /// Advance adjustment in em units (normally negative for tight pairs).
    pub x_advance: f32,
}

/// GPOS `PairPos` kerning recovered from a font, in em units.
///
/// Supports `PairPos` format 1 (explicit pairs) and format 2 (class pairs),
/// horizontal `xAdvance` only.  `kern` table pairs are a separate fallback in
/// [`FontParser`]; a GPOS pair always wins over a `kern` pair.
#[derive(Debug, Clone, Default)]
pub struct GposKerning {
    pairs: HashMap<(u16, u16), f32>,
    classes: Vec<ClassKerning>,
}

impl GposKerning {
    /// Number of explicit format-1 pairs recovered.
    #[must_use]
    pub fn pair_count(&self) -> usize {
        self.pairs.len()
    }

    /// Number of format-2 class subtables recovered.
    #[must_use]
    pub fn class_table_count(&self) -> usize {
        self.classes.len()
    }

    /// Whether any kerning was recovered from `GPOS`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty() && self.classes.is_empty()
    }

    /// The GPOS adjustment for a glyph pair in em units, if the pair is
    /// covered by any lookup.
    ///
    /// A covered pair with a zero adjustment returns `Some(0.0)`: it still
    /// overrides a legacy `kern` value.
    #[must_use]
    pub fn pair(&self, left: u16, right: u16) -> Option<f32> {
        if let Some(value) = self.pairs.get(&(left, right)) {
            return Some(*value);
        }
        for table in &self.classes {
            if !table.coverage.contains(left) {
                continue;
            }
            let class1 = table.class1.class_of(left) as usize;
            let class2 = table.class2.class_of(right) as usize;
            if class1 >= table.class1_count || class2 >= table.class2_count {
                continue;
            }
            let index = class1 * table.class2_count + class2;
            if let Some(value) = table.values.get(index).copied().flatten() {
                return Some(value);
            }
        }
        None
    }

    /// Every explicit pair, as a list.  Used by tests and by callers that want
    /// to interop with a plain pair table.
    #[must_use]
    pub fn explicit_pairs(&self) -> Vec<GposPair> {
        let mut pairs: Vec<GposPair> = self
            .pairs
            .iter()
            .map(|(&(left, right), &x_advance)| GposPair {
                left,
                right,
                x_advance,
            })
            .collect();
        pairs.sort_by_key(|pair| (pair.left, pair.right));
        pairs
    }
}

/// One `PairPos` format-2 class subtable.
#[derive(Debug, Clone)]
struct ClassKerning {
    coverage: Coverage,
    class1: ClassDef,
    class2: ClassDef,
    class1_count: usize,
    class2_count: usize,
    /// `class1_count * class2_count` advance adjustments; `None` when the
    /// record has no `xAdvance` field.
    values: Vec<Option<f32>>,
}

/// A GPOS coverage table, normalised to inclusive glyph ranges.
#[derive(Debug, Clone, Default)]
struct Coverage {
    ranges: Vec<(u16, u16)>,
}

impl Coverage {
    fn parse(data: &[u8], offset: usize) -> Option<Self> {
        let format = u16_at(data, offset)?;
        match format {
            1 => {
                let count = u16_at(data, offset + 2)? as usize;
                let mut ranges = Vec::with_capacity(count);
                let mut pos = offset + 4;
                for _ in 0..count {
                    let glyph = u16_at(data, pos)?;
                    ranges.push((glyph, glyph));
                    pos += 2;
                }
                Some(Self { ranges })
            }
            2 => {
                let count = u16_at(data, offset + 2)? as usize;
                let mut ranges = Vec::with_capacity(count);
                let mut pos = offset + 4;
                for _ in 0..count {
                    let start = u16_at(data, pos)?;
                    let end = u16_at(data, pos + 2)?;
                    ranges.push((start, end.max(start)));
                    pos += 6; // start, end, startCoverageIndex
                }
                Some(Self { ranges })
            }
            _ => None,
        }
    }

    fn contains(&self, glyph: u16) -> bool {
        self.ranges
            .iter()
            .any(|&(start, end)| glyph >= start && glyph <= end)
    }

    /// The `index`-th covered glyph, in coverage order.
    fn glyph_at(&self, index: usize) -> Option<u16> {
        let mut remaining = index;
        for &(start, end) in &self.ranges {
            let count = usize::from(end - start) + 1;
            if remaining < count {
                return Some(start + remaining as u16);
            }
            remaining -= count;
        }
        None
    }
}

/// A GPOS class definition table.
#[derive(Debug, Clone, Default)]
struct ClassDef {
    ranges: Vec<(u16, u16, u16)>,
}

impl ClassDef {
    fn parse(data: &[u8], offset: usize) -> Option<Self> {
        let format = u16_at(data, offset)?;
        match format {
            1 => {
                let start = u16_at(data, offset + 2)?;
                let count = u16_at(data, offset + 4)? as usize;
                let mut ranges = Vec::with_capacity(count);
                let mut pos = offset + 6;
                for index in 0..count {
                    let class = u16_at(data, pos)?;
                    let glyph = start.saturating_add(index as u16);
                    ranges.push((glyph, glyph, class));
                    pos += 2;
                }
                Some(Self { ranges })
            }
            2 => {
                let count = u16_at(data, offset + 2)? as usize;
                let mut ranges = Vec::with_capacity(count);
                let mut pos = offset + 4;
                for _ in 0..count {
                    let start = u16_at(data, pos)?;
                    let end = u16_at(data, pos + 2)?;
                    let class = u16_at(data, pos + 4)?;
                    ranges.push((start, end.max(start), class));
                    pos += 6;
                }
                Some(Self { ranges })
            }
            _ => None,
        }
    }

    /// The class of `glyph`; class 0 is the default when no range matches.
    fn class_of(&self, glyph: u16) -> u16 {
        self.ranges
            .iter()
            .find(|&&(start, end, _)| glyph >= start && glyph <= end)
            .map_or(0, |&(_, _, class)| class)
    }
}

/// TrueType/OpenType font parser.
///
/// Exposes glyph outlines (normalised to em units), advances, vertical metrics,
/// per-glyph ink bounds, `kern` format 0 kerning and GPOS `PairPos` kerning
/// (formats 1 and 2, horizontal advance only).  Shaping (ligatures, mark
/// attachment, complex scripts) is out of scope.
pub struct FontParser {
    data: Vec<u8>,
    tables: HashMap<[u8; 4], TableRecord>,
    index_to_loc_format: i16,
    pub units_per_em: u16,
    metrics: FontMetrics,
    /// `kern` format 0 pairs in em units, keyed by glyph indices.
    kerning: HashMap<(u16, u16), f32>,
    gpos: GposKerning,
}

impl FontParser {
    /// Parse a font from raw bytes.
    ///
    /// Reads the table directory, `head`, `loca`, `hhea`, `kern` and `GPOS`.  A
    /// missing optional table is not an error; missing `head`/`loca` is.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Unsupported`] for a valid font whose outlines this
    /// parser cannot read (CFF/CFF2 outlines, including CFF2 variable fonts),
    /// and [`ParseError::Parse`] for malformed bytes or a missing required
    /// table.  Variable TrueType fonts (`fvar` + `glyf`) are accepted and their
    /// default instance is used; variation axes are not applied.
    pub fn new(data: Vec<u8>) -> Result<Self, ParseError> {
        let mut parser = FontParser {
            data,
            tables: HashMap::new(),
            index_to_loc_format: 0,
            units_per_em: 0,
            metrics: FontMetrics::default(),
            kerning: HashMap::new(),
            gpos: GposKerning::default(),
        };
        parser
            .read_offset_and_directory()
            .map_err(|e| ParseError::Parse(e.to_string()))?;
        parser.check_outline_support()?;
        parser
            .read_head()
            .map_err(|e| ParseError::Parse(e.to_string()))?;
        parser
            .read_loca()
            .map_err(|e| ParseError::Parse(e.to_string()))?;
        parser
            .read_hhea()
            .map_err(|e| ParseError::Parse(e.to_string()))?;
        parser.read_kern();
        parser.read_gpos();
        Ok(parser)
    }

    /// Rejects valid fonts whose outlines this parser does not understand.
    fn check_outline_support(&self) -> Result<(), ParseError> {
        if !self.tables.contains_key(b"glyf") {
            let reason = if self.tables.contains_key(b"CFF2") {
                "CFF2 variable-font outlines are not supported; ship a static TrueType face"
            } else if self.tables.contains_key(b"CFF ") {
                "CFF (OpenType PostScript) outlines are not supported; use a TrueType (glyf) face"
            } else {
                "the font has no TrueType 'glyf' table"
            };
            return Err(ParseError::Unsupported(reason.to_owned()));
        }
        Ok(())
    }

    /// Vertical metrics in em units (see [`FontMetrics`]).
    pub fn metrics(&self) -> FontMetrics {
        self.metrics
    }

    /// Whether GPOS pair kerning was recovered from the font.
    pub fn has_gpos_kerning(&self) -> bool {
        !self.gpos.is_empty()
    }

    /// The recovered GPOS pair kerning (formats 1 and 2).
    pub fn gpos_kerning(&self) -> &GposKerning {
        &self.gpos
    }

    /// The GPOS advance adjustment for a character pair, in em units.
    ///
    /// `None` means the pair is not covered by GPOS; a covered pair returns the
    /// adjustment, which may be zero.
    pub fn get_gpos_kerning(&self, left: char, right: char) -> Option<f32> {
        let l = self.get_glyph_index(left);
        let r = self.get_glyph_index(right);
        self.gpos.pair(l, r)
    }

    /// Whether a `kern` table was found and parsed.
    pub fn has_kern_table(&self) -> bool {
        !self.kerning.is_empty()
    }

    /// All parsed `kern` format 0 pairs, in em units, keyed by glyph indices.
    pub fn kerning_pairs(&self) -> &HashMap<(u16, u16), f32> {
        &self.kerning
    }

    /// Kerning adjustment between two characters in em units (0.0 if absent).
    ///
    /// GPOS pair kerning wins over a legacy `kern` pair; a pair covered by
    /// GPOS with a zero adjustment suppresses the `kern` value.
    pub fn get_kerning(&self, left: char, right: char) -> f32 {
        let l = self.get_glyph_index(left);
        let r = self.get_glyph_index(right);
        self.gpos
            .pair(l, r)
            .or_else(|| self.kerning.get(&(l, r)).copied())
            .unwrap_or(0.0)
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
            .ok_or_else(|| std::io::Error::other("no head"))?;
        let mut cur = std::io::Cursor::new(
            &self.data[rec.offset as usize..rec.offset as usize + rec.length as usize],
        );
        cur.set_position(18);
        self.units_per_em = read_u16_be(&mut cur)?;
        cur.set_position(36);
        let x_min = read_i16_be(&mut cur)? as f32;
        let y_min = read_i16_be(&mut cur)? as f32;
        let x_max = read_i16_be(&mut cur)? as f32;
        let y_max = read_i16_be(&mut cur)? as f32;
        cur.set_position(50);
        self.index_to_loc_format = read_i16_be(&mut cur)?;

        self.metrics.units_per_em = self.units_per_em;
        let upm = self.units_per_em.max(1) as f32;
        self.metrics.bbox = GlyphBounds {
            left: x_min / upm,
            bottom: y_min / upm,
            right: x_max / upm,
            top: y_max / upm,
        };
        Ok(())
    }

    fn read_loca(&self) -> std::io::Result<()> {
        if !self.tables.contains_key(b"loca") {
            return Err(std::io::Error::other("no loca"));
        }
        Ok(())
    }

    /// Read `hhea` ascender/descender/lineGap into [`Self::metrics`].
    ///
    /// A missing table leaves the fields at zero; it is not an error.
    fn read_hhea(&mut self) -> std::io::Result<()> {
        let Some(rec) = self.tables.get(b"hhea") else {
            return Ok(());
        };
        let end = (rec.offset as usize).saturating_add(rec.length as usize);
        if end > self.data.len() || rec.length < 10 {
            return Ok(());
        }
        let mut cur = std::io::Cursor::new(&self.data[rec.offset as usize..end]);
        cur.set_position(4);
        let ascender = read_i16_be(&mut cur)?;
        let descender = read_i16_be(&mut cur)?;
        let line_gap = read_i16_be(&mut cur)?;
        let upm = self.units_per_em.max(1) as f32;
        self.metrics.ascender = ascender as f32 / upm;
        self.metrics.descender = descender as f32 / upm;
        self.metrics.line_gap = line_gap as f32 / upm;
        Ok(())
    }

    /// Parse `kern` format 0 horizontal pairs into [`Self::kerning`].
    ///
    /// Invalid or unsupported subtables are skipped; never panics.
    fn read_kern(&mut self) {
        let Some(rec) = self.tables.get(b"kern") else {
            return;
        };
        let start = rec.offset as usize;
        let end = start
            .saturating_add(rec.length as usize)
            .min(self.data.len());
        if end <= start + 4 {
            return;
        }
        let data = &self.data[start..end];
        let version = u16::from_be_bytes([data[0], data[1]]);
        if version != 0 {
            // Apple `kern` version 1 is not supported.
            return;
        }
        let num_tables = u16::from_be_bytes([data[2], data[3]]) as usize;
        let upm = self.units_per_em.max(1) as f32;
        let mut pos = 4usize;
        for _ in 0..num_tables {
            if pos + 6 > data.len() {
                break;
            }
            let length = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            let coverage = u16::from_be_bytes([data[pos + 4], data[pos + 5]]);
            let format = coverage >> 8;
            let horizontal = coverage & 0x1 != 0;
            // Format 0 subtable: 6-byte header + 8-byte format header + pairs.
            if format == 0 && horizontal && pos + 14 <= data.len() {
                let n_pairs = u16::from_be_bytes([data[pos + 6], data[pos + 7]]) as usize;
                let pairs_start = pos + 14;
                if pairs_start + n_pairs * 6 <= data.len() {
                    for i in 0..n_pairs {
                        let p = pairs_start + i * 6;
                        let left = u16::from_be_bytes([data[p], data[p + 1]]);
                        let right = u16::from_be_bytes([data[p + 2], data[p + 3]]);
                        let value = i16::from_be_bytes([data[p + 4], data[p + 5]]) as f32;
                        self.kerning.insert((left, right), value / upm);
                    }
                }
            }
            if length < 6 {
                break;
            }
            pos += length;
        }
    }

    /// Parse GPOS `PairPos` subtables (format 1 and 2, horizontal advance).
    ///
    /// Every type-2 lookup in the lookup list is read, no matter which script
    /// or feature references it: a `PairPos` lookup is pair kerning by
    /// definition, and fonts routinely share one lookup between scripts.
    /// Malformed or unrecognised subtables are skipped; this never panics.
    fn read_gpos(&mut self) {
        let Some(rec) = self.tables.get(b"GPOS") else {
            return;
        };
        let start = rec.offset as usize;
        let end = start
            .saturating_add(rec.length as usize)
            .min(self.data.len());
        if end <= start + 10 {
            return;
        }
        let data = &self.data[start..end];
        let upm = self.units_per_em.max(1) as f32;
        let Some(lookup_list) = u16_at(data, 8).map(usize::from) else {
            return;
        };
        let Some(lookup_count) = u16_at(data, lookup_list).map(usize::from) else {
            return;
        };
        let mut gpos = GposKerning::default();
        for index in 0..lookup_count {
            let Some(lookup) = u16_at(data, lookup_list + 2 + index * 2).map(usize::from) else {
                break;
            };
            let lookup = lookup_list + lookup;
            if u16_at(data, lookup) != Some(2) {
                continue;
            }
            let Some(sub_count) = u16_at(data, lookup + 4).map(usize::from) else {
                continue;
            };
            for sub_index in 0..sub_count {
                let Some(sub) = u16_at(data, lookup + 6 + sub_index * 2).map(usize::from) else {
                    break;
                };
                let sub = lookup + sub;
                if let Some(parsed) = parse_pair_pos(data, sub, upm) {
                    gpos.pairs.extend(parsed.pairs);
                    gpos.classes.extend(parsed.classes);
                }
            }
        }
        if !gpos.is_empty() {
            log::debug!(
                "GPOS pair kerning: {} explicit pairs, {} class tables",
                gpos.pair_count(),
                gpos.class_table_count()
            );
        }
        self.gpos = gpos;
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

        self.decode_simple_contours(&mut cur, number_of_contours as u16)
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

    /// Outline of the glyph at `glyph_index` (0 = the `.notdef` glyph).
    pub fn get_glyph_outline_by_index(&self, glyph_index: u16) -> Vec<GlyphCommand> {
        self.parse_glyph_recursive(glyph_index, 0)
    }

    pub fn get_glyph_advance(&self, c: char) -> f32 {
        self.get_glyph_advance_by_index(self.get_glyph_index(c))
    }

    /// Advance width of the glyph at `glyph_index`, in em units.
    pub fn get_glyph_advance_by_index(&self, idx: u16) -> f32 {
        let hmtx_rec = match self.tables.get(b"hmtx") {
            Some(r) => r,
            None => return 0.0,
        };
        let hhea_rec = match self.tables.get(b"hhea") {
            Some(r) => r,
            None => return 0.0,
        };
        if hhea_rec.offset as usize + 36 > self.data.len() {
            return 0.0;
        }
        let mut cur = std::io::Cursor::new(&self.data);
        cur.set_position(hhea_rec.offset as u64 + 34);
        let num_h = read_u16_be(&mut cur).unwrap_or(0);
        if num_h == 0 {
            return 0.0;
        }
        let adv = if idx < num_h {
            cur.set_position(hmtx_rec.offset as u64 + (idx as u64 * 4));
            read_u16_be(&mut cur).unwrap_or(0)
        } else {
            cur.set_position(hmtx_rec.offset as u64 + ((num_h as u64 - 1) * 4));
            read_u16_be(&mut cur).unwrap_or(0)
        };
        adv as f32 / self.units_per_em.max(1) as f32
    }

    /// Ink bounds of a character from its `glyf` header, in em units.
    ///
    /// Returns `None` for an empty glyph (for example a space) or when the
    /// `glyf`/`loca` tables are unavailable.
    pub fn get_glyph_bounds(&self, c: char) -> Option<GlyphBounds> {
        self.get_glyph_bounds_by_index(self.get_glyph_index(c))
    }

    /// Ink bounds of the glyph at `glyph_index` from its `glyf` header, in em.
    pub fn get_glyph_bounds_by_index(&self, glyph_index: u16) -> Option<GlyphBounds> {
        let (abs_off0, abs_off1) = self.glyf_offset(glyph_index)?;
        if abs_off0 >= abs_off1 || abs_off0 as usize + 10 > self.data.len() {
            return None;
        }
        let glyf = &self.data[abs_off0 as usize..abs_off1 as usize];
        if glyf.len() < 10 {
            return None;
        }
        let x_min = i16::from_be_bytes([glyf[2], glyf[3]]) as f32;
        let y_min = i16::from_be_bytes([glyf[4], glyf[5]]) as f32;
        let x_max = i16::from_be_bytes([glyf[6], glyf[7]]) as f32;
        let y_max = i16::from_be_bytes([glyf[8], glyf[9]]) as f32;
        let upm = self.units_per_em.max(1) as f32;
        Some(GlyphBounds {
            left: x_min / upm,
            bottom: y_min / upm,
            right: x_max / upm,
            top: y_max / upm,
        })
    }
}

// ── Free helpers ──────────────────────────────────────────────────────────────

/// Reads a big-endian `u16` at `offset`, or `None` when it is out of bounds.
fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Reads a big-endian `i16` at `offset`, or `None` when it is out of bounds.
fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    u16_at(data, offset).map(|value| value as i16)
}

/// Size in bytes of a GPOS value record with `format` bits set.
///
/// Every defined bit (`xPlacement` .. `yAdvanceDevice`) adds one two-byte
/// field.  Higher bits are reserved and ignored.
fn value_record_size(format: u16) -> usize {
    (format & 0x00FF).count_ones() as usize * 2
}

/// Reads `ValueRecord.xAdvance` (bit `0x0004`) when the record has one.
fn read_x_advance(data: &[u8], offset: usize, format: u16) -> Option<i16> {
    let mut pos = offset;
    if format & 0x0001 != 0 {
        pos = pos.checked_add(2)?;
    }
    if format & 0x0002 != 0 {
        pos = pos.checked_add(2)?;
    }
    if format & 0x0004 == 0 {
        return None;
    }
    i16_at(data, pos)
}

/// A `PairPos` subtable parsed into explicit pairs and class tables.
#[derive(Default)]
struct ParsedPairPos {
    pairs: HashMap<(u16, u16), f32>,
    classes: Vec<ClassKerning>,
}

/// Parses one GPOS `PairPos` subtable (format 1 or 2).
fn parse_pair_pos(data: &[u8], offset: usize, upm: f32) -> Option<ParsedPairPos> {
    let format = u16_at(data, offset)?;
    let coverage_offset = usize::from(u16_at(data, offset + 2)?);
    let value_format1 = u16_at(data, offset + 4)?;
    let value_format2 = u16_at(data, offset + 6)?;
    let coverage = Coverage::parse(data, offset.checked_add(coverage_offset)?)?;
    let size1 = value_record_size(value_format1);
    let size2 = value_record_size(value_format2);
    let mut parsed = ParsedPairPos::default();
    match format {
        1 => {
            let pair_set_count = usize::from(u16_at(data, offset + 8)?);
            for pair_set_index in 0..pair_set_count {
                let set_offset = usize::from(u16_at(data, offset + 10 + pair_set_index * 2)?);
                let set = offset.checked_add(set_offset)?;
                let Some(left) = coverage.glyph_at(pair_set_index) else {
                    continue;
                };
                let count = usize::from(u16_at(data, set)?);
                let mut pos = set.checked_add(2)?;
                for _ in 0..count {
                    let right = u16_at(data, pos)?;
                    if let Some(value) = read_x_advance(data, pos + 2, value_format1) {
                        parsed.pairs.insert((left, right), f32::from(value) / upm);
                    }
                    pos = pos.checked_add(2 + size1 + size2)?;
                }
            }
        }
        2 => {
            let class1_offset = usize::from(u16_at(data, offset + 8)?);
            let class2_offset = usize::from(u16_at(data, offset + 10)?);
            let class1_count = usize::from(u16_at(data, offset + 12)?);
            let class2_count = usize::from(u16_at(data, offset + 14)?);
            let class1 = ClassDef::parse(data, offset.checked_add(class1_offset)?)?;
            let class2 = ClassDef::parse(data, offset.checked_add(class2_offset)?)?;
            let mut values = Vec::with_capacity(class1_count.saturating_mul(class2_count));
            let mut pos = offset.checked_add(16)?;
            for _ in 0..class1_count.saturating_mul(class2_count) {
                values.push(read_x_advance(data, pos, value_format1).map(|v| f32::from(v) / upm));
                pos = pos.checked_add(size1 + size2)?;
            }
            parsed.classes.push(ClassKerning {
                coverage,
                class1,
                class2,
                class1_count,
                class2_count,
                values,
            });
        }
        _ => return None,
    }
    Some(parsed)
}

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
        // Resolve from the crate manifest so the test works no matter which
        // directory `cargo test` is invoked from.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let paths = [
            manifest.join("../../assets/fonts/Roboto-Regular.ttf"),
            std::path::PathBuf::from("assets/fonts/Roboto-Regular.ttf"),
            std::path::PathBuf::from("GUIMaker/assets/fonts/Roboto-Regular.ttf"),
        ];
        for p in &paths {
            if let Ok(bytes) = std::fs::read(p) {
                return FontParser::new(bytes).ok();
            }
        }
        None
    }

    #[test]
    fn roboto_vertical_metrics_match_the_file() {
        let Some(parser) = load_roboto() else { return };
        let m = parser.metrics();
        assert_eq!(m.units_per_em, 2048);
        // Values read directly from Roboto-Regular.ttf's `hhea` table.
        assert!((m.ascender - 1900.0 / 2048.0).abs() < 1e-6);
        assert!((m.descender - (-500.0 / 2048.0)).abs() < 1e-6);
        assert!((m.line_gap - 0.0).abs() < 1e-6);
        // Global `head` bounding box.
        assert!((m.bbox.left - (-980.0 / 2048.0)).abs() < 1e-6);
        assert!((m.bbox.top - (2163.0 / 2048.0)).abs() < 1e-6);
    }

    #[test]
    fn roboto_kern_pairs_kerning() {
        let Some(parser) = load_roboto() else { return };
        if !parser.has_kern_table() {
            return;
        }
        let av = parser.get_kerning('A', 'V');
        let to = parser.get_kerning('T', 'o');
        assert!((av - (-87.0 / 2048.0)).abs() < 1e-6, "A/V kern = {av}");
        assert!((to - (-99.0 / 2048.0)).abs() < 1e-6, "T/o kern = {to}");
    }

    #[test]
    fn glyph_bounds_of_o_enclose_its_hole() {
        let Some(parser) = load_roboto() else { return };
        let Some(b) = parser.get_glyph_bounds('o') else {
            panic!("'o' must have ink bounds");
        };
        // Values read directly from the `glyf` header of Roboto's 'o'.
        assert!((b.left - 97.0 / 2048.0).abs() < 1e-6);
        assert!((b.bottom - (-21.0 / 2048.0)).abs() < 1e-6);
        assert!((b.right - 1066.0 / 2048.0).abs() < 1e-6);
        assert!((b.top - 1102.0 / 2048.0).abs() < 1e-6);
        // 'o' has an outer and an inner (hole) contour.
        let outlines = parser.get_glyph_outline('o');
        let contours = outlines
            .iter()
            .filter(|c| matches!(c, GlyphCommand::MoveTo(..)))
            .count();
        assert!(
            contours >= 2,
            "'o' must have a hole (2 contours), got {contours}"
        );
        assert!(
            b.left < 0.25 && b.right > 0.45,
            "hole must sit inside the ink"
        );
    }

    #[test]
    fn gpos_pair_kerning_is_parsed() {
        let Some(parser) = load_roboto() else { return };
        assert!(parser.has_gpos_kerning(), "Roboto has a GPOS table");
        let av = parser
            .get_gpos_kerning('A', 'V')
            .expect("Roboto GPOS must cover A/V");
        let to = parser
            .get_gpos_kerning('T', 'o')
            .expect("Roboto GPOS must cover T/o");
        assert!(av < 0.0, "A/V must tighten, got {av}");
        assert!(to < 0.0, "T/o must tighten, got {to}");
        // The resolved pair is the GPOS value, not the legacy `kern` one.
        assert!((parser.get_kerning('A', 'V') - av).abs() < 1e-6);
    }

    #[test]
    fn cff_outlines_are_a_typed_unsupported_error() {
        let mut data = vec![0, 1, 0, 0];
        data.extend(&1u16.to_be_bytes()); // numTables
        data.extend(&[0; 6]); // searchRange, entrySelector, rangeShift
        data.extend(b"CFF ");
        data.extend(&0u32.to_be_bytes()); // checksum
        data.extend(&28u32.to_be_bytes()); // offset
        data.extend(&0u32.to_be_bytes()); // length
        assert!(matches!(
            FontParser::new(data),
            Err(ParseError::Unsupported(_))
        ));
    }

    #[test]
    fn a_font_without_glyf_is_typed_as_unsupported() {
        let mut data = vec![0, 1, 0, 0];
        data.extend(&0u16.to_be_bytes()); // numTables
        data.extend(&[0; 6]);
        assert!(matches!(
            FontParser::new(data),
            Err(ParseError::Unsupported(_))
        ));
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
