//! A small deterministic hasher for layout memoisation and stable paths.
//!
//! `std`'s default hasher changes with compiler versions; this FNV-1a variant
//! is fixed, so a view hashes to the same value on every run and a repeated
//! build reuses exactly the same layout entries.

#![allow(clippy::cast_possible_truncation)] // reason: string lengths are far below u32::MAX
/// A deterministic FNV-1a hasher.
#[derive(Clone, Copy, Debug)]
pub struct Fnv(pub u64);

impl Default for Fnv {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    /// Starts a hash.
    #[must_use]
    pub const fn new() -> Self {
        Self(Self::OFFSET)
    }

    /// Writes one byte.
    pub fn write_u8(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(Self::PRIME);
    }

    /// Writes a `u32`.
    pub fn write_u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    /// Writes a `u64`.
    pub fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    /// Writes an `f32` by bit pattern, so `-0.0` and `0.0` differ.
    pub fn write_f32(&mut self, value: f32) {
        self.write_u32(value.to_bits());
    }

    /// Writes a boolean.
    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    /// Writes a string.
    pub fn write_str(&mut self, value: &str) {
        self.write_u32(value.len() as u32);
        for byte in value.as_bytes() {
            self.write_u8(*byte);
        }
    }

    /// The finished hash.
    #[must_use]
    pub const fn finish(self) -> u64 {
        self.0
    }
}

/// Hashes a string into a stable path component.
#[must_use]
pub fn hash_str(value: &str) -> u64 {
    let mut hash = Fnv::new();
    hash.write_str(value);
    hash.finish()
}

#[cfg(test)]
mod tests {
    use super::{hash_str, Fnv};

    #[test]
    fn hashes_are_deterministic_and_distinct() {
        assert_eq!(hash_str("start"), hash_str("start"));
        assert_ne!(hash_str("start"), hash_str("Start"));
        let mut ordered = Fnv::new();
        ordered.write_u32(1);
        ordered.write_u32(2);
        let mut reversed = Fnv::new();
        reversed.write_u32(2);
        reversed.write_u32(1);
        assert_ne!(ordered.finish(), reversed.finish());
    }
}
