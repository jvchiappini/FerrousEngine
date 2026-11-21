//! Byte budgets and cheap size estimates for the font caches.
//!
//! The maps here are keyed by small integer/char keys and hold plain data; the
//! `+ 8` term covers the `FxHashMap` control byte and the free-slot slack.  It
//! is an estimate, not an allocator measurement, but it is stable and makes the
//! budget check independent of the hasher's internal layout.

/// Default per-atlas budget for the layout glyph/metrics cache (16 MiB).
///
/// One `GlyphLayout` is 64 bytes and one `TextLineMetrics` is 32 bytes, so
/// 16 MiB holds roughly 250 000 cached glyphs (or the layout of a few very long
/// documents) before the least recently used entry is evicted.
pub const LAYOUT_CACHE_BUDGET_BYTES: u64 = 16 * 1024 * 1024;

/// Maximum number of bytes the per-atlas glyph tables (metrics, glyph indices,
/// `kern` and GPOS kerning) may occupy before a bake is refused.
///
/// These tables are not a cache: evicting a glyph would change the atlas the
/// caller asked for.  A hard, typed rejection is safer than an unbounded bake.
pub const ATLAS_TABLE_BUDGET_BYTES: u64 = 16 * 1024 * 1024;

/// Estimated bytes of a hash map with `len` entries of `K` keys and `V` values.
#[must_use]
pub fn map_bytes<K, V>(len: usize) -> u64 {
    len as u64 * (std::mem::size_of::<K>() + std::mem::size_of::<V>() + 8) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_entry_costs_key_plus_value_plus_overhead() {
        assert_eq!(map_bytes::<char, [f32; 4]>(1), (4 + 16 + 8) as u64);
        assert_eq!(map_bytes::<char, [f32; 4]>(0), 0);
    }
}
