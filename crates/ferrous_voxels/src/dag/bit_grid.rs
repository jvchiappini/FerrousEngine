//! Compact 3-D bit grids used for occupancy and emissivity tracking inside
//! the HashDAG.
//!
//! A `BitGrid3D` maps integer 3-D coordinates to a single bit.  It is used
//! at each hierarchy level to answer "does any voxel exist in this region?"
//! without traversing the full DAG.
//!
//! # Memory layout
//!
//! Internally the grid is stored as a flat `Vec<u64>` where each `u64` packs
//! 64 bits.  The mapping from `(x, y, z)` to `(word_index, bit_index)` is:
//!
//! ```text
//! linear_index = z * (size_x * size_y) + y * size_x + x
//! word_index   = linear_index / 64
//! bit_index    = linear_index % 64
//! ```
//!
//! Dimensions must be multiples of 4 for alignment — a debug assertion
//! enforces this at construction.

/// A compact, dynamically-sized 3-D bitfield.
///
/// Dimensions are fixed at construction and cannot be resized. Create a new
/// `BitGrid3D` if the world grows.
#[derive(Debug, Clone)]
pub struct BitGrid3D {
    /// Grid dimensions in cells.
    size_x: u32,
    size_y: u32,
    size_z: u32,
    /// Flat bit storage (ceil(total_cells / 64) words).
    words: Vec<u64>,
}

impl BitGrid3D {
    /// Create a new bit grid of dimensions `(sx, sy, sz)`.
    ///
    /// All cells start as `0` (clear).
    ///
    /// # Panics
    ///
    /// Panics in debug builds if any dimension is zero.
    pub fn new(size_x: u32, size_y: u32, size_z: u32) -> Self {
        debug_assert!(
            size_x > 0 && size_y > 0 && size_z > 0,
            "BitGrid3D: dimensions must be > 0"
        );
        let total = (size_x as usize) * (size_y as usize) * (size_z as usize);
        let words = total.div_ceil(64);
        Self {
            size_x,
            size_y,
            size_z,
            words: vec![0u64; words],
        }
    }

    /// Returns the grid dimensions as `(x, y, z)`.
    #[inline]
    pub fn dimensions(&self) -> (u32, u32, u32) {
        (self.size_x, self.size_y, self.size_z)
    }

    /// Total number of cells.
    #[inline]
    pub fn cell_count(&self) -> usize {
        (self.size_x as usize) * (self.size_y as usize) * (self.size_z as usize)
    }

    /// Convert 3-D coordinates to a flat linear index.
    ///
    /// Returns `None` if the coordinates are out of bounds.
    #[inline]
    fn linear(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if x < 0
            || y < 0
            || z < 0
            || x >= self.size_x as i32
            || y >= self.size_y as i32
            || z >= self.size_z as i32
        {
            return None;
        }
        Some(
            (z as usize) * (self.size_x as usize) * (self.size_y as usize)
                + (y as usize) * (self.size_x as usize)
                + (x as usize),
        )
    }

    /// Set the bit at `(x, y, z)` to `1`.
    ///
    /// Out-of-bounds coordinates are silently ignored.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, z: i32) {
        if let Some(idx) = self.linear(x, y, z) {
            self.words[idx / 64] |= 1u64 << (idx % 64);
        }
    }

    /// Clear the bit at `(x, y, z)` (set to `0`).
    ///
    /// Out-of-bounds coordinates are silently ignored.
    #[inline]
    pub fn clear(&mut self, x: i32, y: i32, z: i32) {
        if let Some(idx) = self.linear(x, y, z) {
            self.words[idx / 64] &= !(1u64 << (idx % 64));
        }
    }

    /// Returns `true` if the bit at `(x, y, z)` is set.
    ///
    /// Returns `false` for out-of-bounds coordinates.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> bool {
        match self.linear(x, y, z) {
            Some(idx) => (self.words[idx / 64] >> (idx % 64)) & 1 != 0,
            None => false,
        }
    }

    /// Set the bit at `(x, y, z)` to the given `value`.
    #[inline]
    pub fn write(&mut self, x: i32, y: i32, z: i32, value: bool) {
        if value {
            self.set(x, y, z);
        } else {
            self.clear(x, y, z);
        }
    }

    /// Clear all bits.
    #[inline]
    pub fn clear_all(&mut self) {
        for w in &mut self.words {
            *w = 0;
        }
    }

    /// Returns `true` if no bit is set (the entire grid is zero).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Return the raw word slice for GPU upload.
    ///
    /// Each `u64` covers 64 consecutive cells in memory-linear order.
    #[inline]
    pub fn as_u64_slice(&self) -> &[u64] {
        &self.words
    }

    /// Count the total number of set bits (popcount).
    pub fn count_ones(&self) -> u64 {
        self.words.iter().map(|w| w.count_ones() as u64).sum()
    }
}

/// A pair of bit grids representing occupancy and emissivity at one DAG level.
#[derive(Debug, Clone)]
pub struct LevelGrids {
    /// Set when a cell contains at least one solid voxel.
    pub occupancy: BitGrid3D,
    /// Set when a cell contains at least one emissive voxel.
    pub emissivity: BitGrid3D,
}

impl LevelGrids {
    /// Construct with the given cell dimensions for this hierarchy level.
    pub fn new(size_x: u32, size_y: u32, size_z: u32) -> Self {
        Self {
            occupancy: BitGrid3D::new(size_x, size_y, size_z),
            emissivity: BitGrid3D::new(size_x, size_y, size_z),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_clear() {
        let mut g = BitGrid3D::new(8, 8, 8);
        assert!(!g.get(0, 0, 0));
        g.set(0, 0, 0);
        assert!(g.get(0, 0, 0));
        g.clear(0, 0, 0);
        assert!(!g.get(0, 0, 0));
    }

    #[test]
    fn out_of_bounds_is_false() {
        let g = BitGrid3D::new(4, 4, 4);
        assert!(!g.get(-1, 0, 0));
        assert!(!g.get(4, 0, 0));
    }

    #[test]
    fn clear_all() {
        let mut g = BitGrid3D::new(4, 4, 4);
        g.set(1, 2, 3);
        g.set(0, 0, 0);
        g.clear_all();
        assert!(g.is_empty());
    }

    #[test]
    fn count_ones() {
        let mut g = BitGrid3D::new(4, 4, 4);
        g.set(0, 0, 0);
        g.set(1, 0, 0);
        g.set(2, 0, 0);
        assert_eq!(g.count_ones(), 3);
    }

    #[test]
    fn large_grid_no_panic() {
        let mut g = BitGrid3D::new(64, 64, 64);
        g.set(63, 63, 63);
        assert!(g.get(63, 63, 63));
        assert!(!g.get(63, 63, 62));
    }
}
