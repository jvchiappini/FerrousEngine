//! Definitions of various TrueType/OpenType table structures used by the
//! font parser.

/// A directory entry in the font file's table directory.
#[derive(Debug, Clone)]
pub struct TableRecord {
    pub tag: [u8; 4],
    pub checksum: u32,
    pub offset: u32,
    pub length: u32,
}
