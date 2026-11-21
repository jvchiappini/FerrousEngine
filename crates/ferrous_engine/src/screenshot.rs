//! Copying a presented frame back to CPU memory.
//!
//! The window surface is configured with `COPY_SRC` when the adapter allows
//! it.  An application asks for one frame with
//! [`AppContext::request_screenshot`](crate::AppContext::request_screenshot)
//! and receives it, tight and in RGBA8, through
//! [`FerrousApp::on_screenshot`](crate::FerrousApp::on_screenshot) after that
//! frame is presented.  A copy that cannot be made (adapter without
//! `COPY_SRC`, a failed map) logs a warning and delivers nothing; it never
//! panics.

use std::sync::mpsc;

/// One presented frame, top-left origin, rows tightly packed, RGBA8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screenshot {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, RGBA in that order.
    pub rgba: Vec<u8>,
}

impl Screenshot {
    /// The raw RGBA8 bytes, `width * height * 4` long.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.rgba
    }
}

/// Row stride required by `copy_texture_to_buffer` for an RGBA8 texture.
#[must_use]
pub fn padded_bytes_per_row(width: u32) -> u32 {
    (width.max(1) * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

/// Whether the surface capabilities allow reading the presented texture.
#[must_use]
pub fn supports_copy_src(caps: &wgpu::SurfaceCapabilities) -> bool {
    caps.usages.contains(wgpu::TextureUsages::COPY_SRC)
}

/// Whether `format` stores BGRA rather than RGBA bytes.
#[must_use]
pub fn is_bgra(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

/// Removes the per-row padding from a mapped copy and converts it to RGBA.
///
/// `padded` must hold at least `padded_bytes_per_row(width) * height` bytes;
/// returns `None` otherwise.  When `bgra` is set, the R and B channels are
/// swapped (the sRGB encoding and the alpha channel are left alone).
#[must_use]
pub fn tight_rgba(padded: &[u8], width: u32, height: u32, bgra: bool) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let stride = padded_bytes_per_row(width) as usize;
    let row_bytes = width as usize * 4;
    let required = stride.checked_mul(height as usize)?;
    if padded.len() < required {
        return None;
    }
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * stride;
        let src = &padded[start..start + row_bytes];
        if bgra {
            for px in src.chunks_exact(4) {
                out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        } else {
            out.extend_from_slice(src);
        }
    }
    Some(out)
}

/// Allocates a staging buffer that can receive one `width`×`height` frame.
pub(crate) fn capture_buffer(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
    let size = u64::from(padded_bytes_per_row(width)) * u64::from(height.max(1));
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Frame Screenshot Staging"),
        size: size.max(1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// Maps `buffer` without blocking the event loop and returns the tight RGBA
/// frame, or `None` after logging a warning.
pub(crate) fn read_capture(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    bgra: bool,
) -> Option<Screenshot> {
    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::Maintain::Wait);
    let result = receiver.recv();
    let rgba = match result {
        Ok(Ok(())) => {
            let mapped = slice.get_mapped_range();
            let tight = tight_rgba(&mapped, width, height, bgra);
            drop(mapped);
            tight
        }
        Ok(Err(error)) => {
            log::warn!("screenshot: buffer map failed: {error}");
            None
        }
        Err(error) => {
            log::warn!("screenshot: mapping result was lost: {error}");
            None
        }
    };
    buffer.unmap();
    rgba.map(|rgba| Screenshot {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_padding_removes_the_alignment_gap() {
        // 3 px = 12 bytes per row, but the copy needs a 256-byte stride.
        let width = 3;
        let height: u32 = 2;
        let stride = padded_bytes_per_row(width) as usize;
        assert_eq!(stride, 256);
        let mut padded = vec![0u8; stride * height as usize];
        for row in 0..height as usize {
            for x in 0..width as usize {
                let at = row * stride + x * 4;
                padded[at..at + 4].copy_from_slice(&[1, 2, 3, 4]);
            }
            // Garbage in the padding must not leak into the result.
            let tail = row * stride + width as usize * 4;
            padded[tail..(row + 1) * stride].fill(0xFF);
        }
        let tight = tight_rgba(&padded, width, height, false).unwrap();
        assert_eq!(tight.len(), width as usize * 4 * height as usize);
        for px in tight.chunks_exact(4) {
            assert_eq!(px, [1, 2, 3, 4]);
        }
    }

    #[test]
    fn bgra_is_swizzled_to_rgba() {
        let width = 2;
        let height: u32 = 1;
        let stride = padded_bytes_per_row(width) as usize;
        let mut padded = vec![0u8; stride];
        padded[0..4].copy_from_slice(&[0x10, 0x20, 0x30, 0xFF]); // B G R A
        padded[4..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x80]);
        let tight = tight_rgba(&padded, width, height, true).unwrap();
        assert_eq!(&tight[0..4], [0x30, 0x20, 0x10, 0xFF]);
        assert_eq!(&tight[4..8], [0x03, 0x02, 0x01, 0x80]);
    }

    #[test]
    fn a_short_buffer_is_refused_instead_of_read_out_of_bounds() {
        assert!(tight_rgba(&[0u8; 16], 3, 2, false).is_none());
        assert!(tight_rgba(&[], 0, 0, false).is_none());
    }

    #[test]
    fn bgra_formats_are_recognised() {
        assert!(is_bgra(wgpu::TextureFormat::Bgra8UnormSrgb));
        assert!(is_bgra(wgpu::TextureFormat::Bgra8Unorm));
        assert!(!is_bgra(wgpu::TextureFormat::Rgba8UnormSrgb));
    }

    #[test]
    fn padded_rows_are_a_multiple_of_256() {
        for width in [1, 63, 64, 65, 1920, 3840] {
            assert_eq!(padded_bytes_per_row(width) % 256, 0);
            assert!(padded_bytes_per_row(width) >= width * 4);
        }
    }
}
