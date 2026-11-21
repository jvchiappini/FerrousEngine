//! Little-endian binary primitives shared by the baked-atlas file format.
//!
//! The format is fixed-layout rather than self-describing: every reader knows
//! the order of the fields it expects.  All reads are bounds-checked and return
//! `None` on a truncated file, which is what lets a corrupt cache entry be
//! ignored instead of panicking.

/// Appends little-endian values to a byte buffer.
pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// An empty writer with room for `capacity` bytes.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    /// Appends one byte.
    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// Appends a `u16`.
    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends a `u32`.
    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends two `u64`s (a `u128` key).
    pub(crate) fn u128(&mut self, value: u128) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Appends an `f32` as its bit pattern.
    pub(crate) fn f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    /// Appends a run of bytes.
    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    /// The buffer written so far.
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Reads little-endian values from a byte slice, or `None` past its end.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    /// A reader over `data`.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// Reads one byte.
    pub(crate) fn u8(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.position)?;
        self.position += 1;
        Some(byte)
    }

    /// Reads a `u16`.
    pub(crate) fn u16(&mut self) -> Option<u16> {
        let bytes = self.bytes(2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Reads a `u32`.
    pub(crate) fn u32(&mut self) -> Option<u32> {
        let bytes = self.bytes(4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads a `u128`.
    pub(crate) fn u128(&mut self) -> Option<u128> {
        let bytes = self.bytes(16)?;
        let mut array = [0u8; 16];
        array.copy_from_slice(bytes);
        Some(u128::from_le_bytes(array))
    }

    /// Reads an `f32`.
    pub(crate) fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }

    /// Reads a fixed-size slice of `len` bytes.
    pub(crate) fn bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(len)?;
        let slice = self.data.get(self.position..end)?;
        self.position = end;
        Some(slice)
    }

    /// Reads a count that must be usable as a table length.
    ///
    /// Counts are attacker/corruption controlled; a cap keeps a bad header from
    /// asking for a huge allocation before the first bounds check fails.
    pub(crate) fn count(&mut self, cap: u32) -> Option<usize> {
        let count = self.u32()?;
        if count > cap {
            return None;
        }
        Some(count as usize)
    }

    /// Whether every byte has been consumed.
    pub(crate) fn is_empty(&self) -> bool {
        self.position == self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_primitive() {
        let mut writer = Writer::with_capacity(0);
        writer.u8(7);
        writer.u16(0xABCD);
        writer.u32(0xDEAD_BEEF);
        writer.f32(-1.5);
        writer.u128(0x0123_4567_89AB_CDEF_0011_2233_4455_6677);
        writer.bytes(b"tail");
        let bytes = writer.into_bytes();

        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u8(), Some(7));
        assert_eq!(reader.u16(), Some(0xABCD));
        assert_eq!(reader.u32(), Some(0xDEAD_BEEF));
        assert_eq!(reader.f32(), Some(-1.5));
        assert_eq!(
            reader.u128(),
            Some(0x0123_4567_89AB_CDEF_0011_2233_4455_6677)
        );
        assert_eq!(reader.bytes(4), Some(&b"tail"[..]));
        assert!(reader.is_empty());
    }

    #[test]
    fn a_truncated_read_returns_none() {
        let mut reader = Reader::new(&[1, 2, 3]);
        assert_eq!(reader.u16(), Some(0x0201));
        assert_eq!(reader.u16(), None);
        assert_eq!(reader.u8(), Some(3));
        assert_eq!(reader.u8(), None);
    }

    #[test]
    fn a_count_over_the_cap_is_rejected() {
        let bytes = u32::MAX.to_le_bytes();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.count(1024), None);
    }
}
