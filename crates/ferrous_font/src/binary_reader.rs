use std::io::{self, Read, Seek, SeekFrom};

/// Read an unsigned 16-bit big-endian value from the cursor.
pub fn read_u16_be<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

/// Read an unsigned 32-bit big-endian value.
pub fn read_u32_be<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

/// Read a signed 16-bit big-endian value.
pub fn read_i16_be<R: Read>(reader: &mut R) -> io::Result<i16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(i16::from_be_bytes(buf))
}

/// Read a signed 32-bit big-endian value.
pub fn read_i32_be<R: Read>(reader: &mut R) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_be_bytes(buf))
}

/// Convenience function to read a u32 at a specific offset in a slice.
pub fn read_u32_be_from_slice(slice: &[u8], offset: usize) -> io::Result<u32> {
    let mut cur = io::Cursor::new(slice);
    cur.seek(SeekFrom::Start(offset as u64))?;
    read_u32_be(&mut cur)
}
// Similar convenience functions can be added as needed.
