//! CPU mip-chain generation and texture options for the Pure2D sprite path.
//!
//! Textures are uploaded as `Rgba8UnormSrgb`, which wgpu decodes to linear
//! light on sample.  Mip levels are generated on the CPU **in linear light on
//! premultiplied values**, then re-encoded to sRGB for storage.  Filtering the
//! sRGB bytes directly would darken every downsampled image; a 2×2
//! black/white checker must become linear 0.5 (sRGB ~188), not sRGB 0.5
//! (128).
//!
//! The chain is deterministic: a box filter with exact area weights in both
//! axes.  Even sizes therefore average exactly 2×2 texels; odd sizes are
//! resampled with the fractional box that covers them (a 3×3 image becomes one
//! texel averaging all nine).  Each level is `max(1, w / 2)` by
//! `max(1, h / 2)`, which is exactly the mip count wgpu accepts for the
//! texture.

/// Filtering used when a sprite texture is sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureFilter2d {
    /// Nearest-neighbour; pixel-art exact.
    #[default]
    Nearest,
    /// Bilinear magnification and minification.
    Linear,
}

/// How [`crate::render::Renderer2d`]-owning renderers upload a sprite texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureOptions2d {
    /// Build and upload a full mip chain (trilinear sampling).
    pub mipmaps: bool,
    /// Filter used for magnification and minification.
    pub filter: TextureFilter2d,
}

impl TextureOptions2d {
    /// Mipmapped, linearly filtered: the production choice for photos.
    #[must_use]
    pub const fn photo() -> Self {
        Self {
            mipmaps: true,
            filter: TextureFilter2d::Linear,
        }
    }

    /// No mipmaps, nearest filtering: the pixel-art choice.
    #[must_use]
    pub const fn pixel_art() -> Self {
        Self {
            mipmaps: false,
            filter: TextureFilter2d::Nearest,
        }
    }
}

impl Default for TextureOptions2d {
    fn default() -> Self {
        Self::photo()
    }
}

/// Why a texture could not be built or registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureError {
    /// A width or height of zero.
    ZeroSize,
    /// The byte slice is not `width * height * 4` bytes.
    SizeMismatch {
        /// Required length.
        expected: usize,
        /// Supplied length.
        actual: usize,
    },
    /// A dimension exceeds the device's `max_texture_dimension_2d`.
    TooLarge {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// Device limit.
        limit: u32,
    },
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroSize => write!(f, "texture width and height must be at least 1"),
            Self::SizeMismatch { expected, actual } => write!(
                f,
                "texture data is {actual} bytes but {expected} are required (w*h*4)"
            ),
            Self::TooLarge {
                width,
                height,
                limit,
            } => write!(
                f,
                "texture {width}x{height} exceeds the device limit of {limit} per dimension"
            ),
        }
    }
}

impl std::error::Error for TextureError {}

/// One mip level: premultiplied, sRGB-encoded RGBA8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipLevel {
    /// Level width in texels.
    pub width: u32,
    /// Level height in texels.
    pub height: u32,
    /// `width * height * 4` premultiplied sRGB bytes.
    pub rgba8_srgb_premultiplied: Vec<u8>,
}

/// A full mip chain, level 0 first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipChain {
    /// Levels, largest first, down to 1×1.
    pub levels: Vec<MipLevel>,
}

impl MipChain {
    /// Total resident bytes of every level.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.levels
            .iter()
            .map(|level| u64::from(level.width) * u64::from(level.height) * 4)
            .sum()
    }

    /// The level at `index`, if it exists.
    #[must_use]
    pub fn level(&self, index: usize) -> Option<&MipLevel> {
        self.levels.get(index)
    }

    /// Number of levels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    /// Whether the chain is empty (never for a valid image).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }
}

/// Builds a deterministic full mip chain from straight-alpha sRGB RGBA8.
///
/// Level 0 is converted to premultiplied linear light and re-encoded to sRGB
/// once.  Every later level is a box-filter average of the previous level in
/// premultiplied linear light.  The byte length must be `width * height * 4`.
///
/// # Errors
///
/// Returns [`TextureError::ZeroSize`] for a zero dimension and
/// [`TextureError::SizeMismatch`] for the wrong byte count.
pub fn build_mip_chain(
    width: u32,
    height: u32,
    rgba8_srgb_straight: &[u8],
) -> Result<MipChain, TextureError> {
    build_chain(width, height, rgba8_srgb_straight, true)
}

/// Builds only mip level 0 from straight-alpha sRGB RGBA8.
///
/// This is the `mipmaps: false` path: a 4K image does not pay for a full box
/// filtered chain that would be discarded before upload.  Level 0 has exactly
/// the same premultiplied sRGB bytes [`build_mip_chain`] produces for its first
/// level, so the sampled texels are identical.
///
/// # Errors
///
/// Returns [`TextureError::ZeroSize`] for a zero dimension and
/// [`TextureError::SizeMismatch`] for the wrong byte count.
pub fn build_level_zero(
    width: u32,
    height: u32,
    rgba8_srgb_straight: &[u8],
) -> Result<MipChain, TextureError> {
    build_chain(width, height, rgba8_srgb_straight, false)
}

/// Shared chain builder; `full` selects every level or level 0 only.
fn build_chain(
    width: u32,
    height: u32,
    rgba8_srgb_straight: &[u8],
    full: bool,
) -> Result<MipChain, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::ZeroSize);
    }
    let expected = width as usize * height as usize * 4;
    if rgba8_srgb_straight.len() != expected {
        return Err(TextureError::SizeMismatch {
            expected,
            actual: rgba8_srgb_straight.len(),
        });
    }

    // Level 0: straight sRGB -> premultiplied linear.
    let mut current = Vec::with_capacity(width as usize * height as usize);
    for texel in rgba8_srgb_straight.chunks_exact(4) {
        let alpha = f32::from(texel[3]) / 255.0;
        current.push([
            srgb_to_linear(texel[0]) * alpha,
            srgb_to_linear(texel[1]) * alpha,
            srgb_to_linear(texel[2]) * alpha,
            alpha,
        ]);
    }

    let mut levels = vec![encode_level(width, height, &current)];
    if !full {
        return Ok(MipChain { levels });
    }
    let mut source_width = width;
    let mut source_height = height;
    while source_width > 1 || source_height > 1 {
        let next_width = (source_width / 2).max(1);
        let next_height = (source_height / 2).max(1);
        let x_weights = axis_weights(source_width, next_width);
        let y_weights = axis_weights(source_height, next_height);
        let mut next = vec![[0.0f32; 4]; next_width as usize * next_height as usize];
        for (y, rows) in y_weights.iter().enumerate() {
            for (x, columns) in x_weights.iter().enumerate() {
                let mut sum = [0.0f32; 4];
                let mut total = 0.0f32;
                for &(sy, wy) in rows {
                    for &(sx, wx) in columns {
                        let weight = wy * wx;
                        let texel = current[sy as usize * source_width as usize + sx as usize];
                        for channel in 0..4 {
                            sum[channel] += texel[channel] * weight;
                        }
                        total += weight;
                    }
                }
                let texel = if total > 0.0 {
                    [
                        sum[0] / total,
                        sum[1] / total,
                        sum[2] / total,
                        sum[3] / total,
                    ]
                } else {
                    [0.0; 4]
                };
                next[y * next_width as usize + x] = texel;
            }
        }
        levels.push(encode_level(next_width, next_height, &next));
        current = next;
        source_width = next_width;
        source_height = next_height;
    }

    Ok(MipChain { levels })
}

/// Exact area weights for one axis: `(source index, weight)` per destination
/// texel.  Even sizes give two equal weights; odd sizes give fractional
/// weights that cover every source texel exactly once.
fn axis_weights(source: u32, destination: u32) -> Vec<Vec<(u32, f32)>> {
    let mut out = Vec::with_capacity(destination as usize);
    let scale = f64::from(source) / f64::from(destination);
    for index in 0..destination {
        let start = f64::from(index) * scale;
        let end = start + scale;
        let first = start.floor() as u32;
        let last = end.ceil() as u32;
        let mut weights = Vec::new();
        for source_index in first..last.min(source) {
            let overlap = (end.min(f64::from(source_index + 1))
                - start.max(f64::from(source_index)))
            .max(0.0);
            if overlap > 0.0 {
                // Normalized to one in this axis.
                #[allow(clippy::cast_possible_truncation)] // weights are tiny
                let weight = (overlap / scale) as f32;
                weights.push((source_index, weight));
            }
        }
        out.push(weights);
    }
    out
}

/// Encodes premultiplied linear texels to a premultiplied sRGB level.
fn encode_level(width: u32, height: u32, texels: &[[f32; 4]]) -> MipLevel {
    let mut data = Vec::with_capacity(texels.len() * 4);
    for texel in texels {
        data.push(linear_to_srgb(texel[0]));
        data.push(linear_to_srgb(texel[1]));
        data.push(linear_to_srgb(texel[2]));
        data.push(linear_to_byte(texel[3]));
    }
    MipLevel {
        width,
        height,
        rgba8_srgb_premultiplied: data,
    }
}

/// sRGB EOTF for one 8-bit channel.
fn srgb_to_linear(value: u8) -> f32 {
    let channel = f32::from(value) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB OETF for one linear channel.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped before the cast
fn linear_to_srgb(value: f32) -> u8 {
    let channel = value.clamp(0.0, 1.0);
    let encoded = if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

/// Linear coverage in `0.0..=1.0` to an 8-bit alpha byte.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped before the cast
fn linear_to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_black_white_checker_becomes_linear_half() {
        // 2x2: black, white / white, black, all opaque.
        let data = [
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
        ];
        let chain = build_mip_chain(2, 2, &data).unwrap();
        assert_eq!(chain.len(), 2);
        let last = chain.level(1).unwrap();
        assert_eq!((last.width, last.height), (1, 1));
        let texel = &last.rgba8_srgb_premultiplied;
        // Linear 0.5 encoded as sRGB is ~188, not the sRGB average 128.
        assert!(
            (i16::from(texel[0]) - 188).abs() <= 1,
            "expected linear 0.5 (~188), got {}",
            texel[0]
        );
        assert_eq!(texel[3], 255);
    }

    #[test]
    fn odd_sizes_cover_every_texel() {
        // 3x3: nine different opaque greys; the single 1x1 texel is their
        // linear average, not a 2x2 crop.
        let values = [0u8, 32, 64, 96, 128, 160, 192, 224, 255];
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(&[value, value, value, 255]);
        }
        let chain = build_mip_chain(3, 3, &data).unwrap();
        assert_eq!(chain.len(), 2, "3x3 has two levels");
        assert_eq!(
            (
                chain.level(1).unwrap().width,
                chain.level(1).unwrap().height
            ),
            (1, 1)
        );
        let linear: f64 = values
            .iter()
            .map(|&value| f64::from(srgb_to_linear(value)))
            .sum::<f64>()
            / 9.0;
        let expected = linear_to_srgb(linear as f32);
        assert!(
            (i16::from(chain.level(1).unwrap().rgba8_srgb_premultiplied[0]) - i16::from(expected))
                .abs()
                <= 1,
            "expected {expected}, got {}",
            chain.level(1).unwrap().rgba8_srgb_premultiplied[0]
        );
    }

    #[test]
    fn a_half_transparent_white_texel_is_premultiplied() {
        let data = [255, 255, 255, 128];
        let chain = build_mip_chain(1, 1, &data).unwrap();
        let texel = &chain.level(0).unwrap().rgba8_srgb_premultiplied;
        // Premultiplied linear 0.502 -> sRGB ~188, alpha stays 128.
        assert!((i16::from(texel[0]) - 188).abs() <= 1, "got {}", texel[0]);
        assert_eq!(texel[3], 128);
    }

    #[test]
    fn level_zero_matches_the_first_level_of_the_full_chain() {
        let data = [
            0u8, 64, 128, 255, 255, 200, 100, 32, 10, 20, 30, 40, 50, 60, 70, 255,
        ];
        let flat = build_level_zero(2, 2, &data).unwrap();
        assert_eq!(flat.len(), 1);
        assert_eq!(flat.bytes(), 2 * 2 * 4);
        let full = build_mip_chain(2, 2, &data).unwrap();
        assert_eq!(flat.level(0), full.level(0));
    }

    #[test]
    fn sizes_are_validated() {
        assert_eq!(build_mip_chain(0, 1, &[]), Err(TextureError::ZeroSize));
        assert_eq!(
            build_mip_chain(2, 2, &[0; 3]),
            Err(TextureError::SizeMismatch {
                expected: 16,
                actual: 3
            })
        );
    }

    #[test]
    fn chain_length_matches_the_wgpu_limit() {
        for (width, height) in [(1, 1), (2, 2), (3, 3), (5, 7), (8, 8), (16, 4)] {
            let data = vec![128u8; width as usize * height as usize * 4];
            let chain = build_mip_chain(width, height, &data).unwrap();
            let limit = (u32::max(width, height).ilog2() + 1) as usize;
            assert_eq!(chain.len(), limit, "{width}x{height}");
        }
    }
}
