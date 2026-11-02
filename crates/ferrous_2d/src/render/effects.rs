//! Layer effects: separable blur, drop shadow and glow.
//!
//! Effects are a property of an offscreen [`Layer2d`](crate::render::Layer2d):
//! the layer's draws render into its texture as usual, then the effect chain
//! runs in order over that texture and the result is composited into the parent
//! with the layer's opacity and
//! [`CompositeMode2d`](crate::render::CompositeMode2d).
//!
//! Every effect is deterministic: the Gaussian kernel is a pure function of the
//! radius (see [`gaussian_kernel`]), there is no randomness and no wall clock,
//! and the same document always produces the same pixels on the same adapter.
//!
//! The chain semantics are "each effect transforms the current image":
//!
//! - [`Effect2d::Blur`] replaces the image with its Gaussian blur.
//! - [`Effect2d::DropShadow`] blurs the image, tints it, offsets it and puts it
//!   behind the image.
//! - [`Effect2d::Glow`] blurs the image, tints it and adds it as light around
//!   the image (an outer glow: the content itself is not lit).
//!
//! Textures are linear and premultiplied throughout, exactly like layer
//! textures.

/// Largest one-sided tap count of the Gaussian kernel.
///
/// The kernel is symmetric, so a blur performs `2 * taps - 1` samples per
/// pixel and direction.  This bounds the cost of a single pass.
pub const MAX_BLUR_TAPS: usize = 76;

// The blur uniform packs the weights four per `vec4`.
const _: () = assert!(MAX_BLUR_TAPS.is_multiple_of(4));

/// Largest finite blur radius supported by [`gaussian_kernel`], in pixels.
///
/// Larger radii are clamped: beyond this point the kernel would need more taps
/// than [`MAX_BLUR_TAPS`] to stay a faithful Gaussian.
pub const MAX_BLUR_RADIUS: f32 = 48.0;

/// Largest number of effects a single layer may carry.
pub const MAX_EFFECTS_PER_LAYER: usize = 8;

/// A normalized, symmetric Gaussian kernel.
///
/// `taps` counts the samples on one side of the centre, including the centre
/// itself: the kernel covers offsets `-(taps - 1)..=(taps - 1)` at one-pixel
/// spacing.  `weights[i]` is the weight of offset `±i`; `weights[taps..]` is
/// zero.  The weights sum to 1 (centre plus both wings), so a blur preserves
/// the total coverage of the image.
///
/// The radius maps to `sigma = radius / 2`, the convention of CSS
/// `box-shadow` and canvas `shadowBlur`, and the kernel spans three standard
/// deviations (`taps = ceil(1.5 * radius) + 1`).  A radius of zero (or any
/// non-finite value) yields the identity kernel: `taps == 1`,
/// `weights[0] == 1`.
#[derive(Debug, Clone, PartialEq)]
pub struct BlurKernel2d {
    /// One-sided tap count, including the centre.
    pub taps: u32,
    /// Weights for offsets `0..MAX_BLUR_TAPS` (zero past `taps`).
    pub weights: [f32; MAX_BLUR_TAPS],
}

impl BlurKernel2d {
    /// The kernel for `radius` pixels.
    ///
    /// `radius` is clamped to `0.0..=`
    /// [`MAX_BLUR_RADIUS`]; a non-finite value is treated as `0.0`.
    #[must_use]
    pub fn new(radius: f32) -> Self {
        let radius = if radius.is_finite() {
            radius.clamp(0.0, MAX_BLUR_RADIUS)
        } else {
            0.0
        };
        let taps = ((radius * 1.5).ceil() as usize + 1).clamp(1, MAX_BLUR_TAPS);
        let mut weights = [0.0_f32; MAX_BLUR_TAPS];
        if taps <= 1 || radius <= 0.0 {
            weights[0] = 1.0;
            return Self { taps: 1, weights };
        }
        let sigma = radius / 2.0;
        let two_sigma_squared = 2.0 * sigma * sigma;
        let mut total = 0.0_f32;
        for (i, slot) in weights.iter_mut().take(taps).enumerate() {
            let offset = i as f32;
            let weight = (-(offset * offset) / two_sigma_squared).exp();
            *slot = weight;
            total += if i == 0 { weight } else { 2.0 * weight };
        }
        if total > 0.0 {
            for weight in weights.iter_mut().take(taps) {
                *weight /= total;
            }
        } else {
            weights[0] = 1.0;
        }
        Self {
            taps: taps as u32,
            weights,
        }
    }

    /// Weight of offset `i`, or zero when `i >= taps`.
    #[must_use]
    pub fn weight(&self, i: usize) -> f32 {
        self.weights.get(i).copied().unwrap_or(0.0)
    }
}

/// The kernel for `radius` pixels; shorthand for [`BlurKernel2d::new`].
#[must_use]
pub fn gaussian_kernel(radius: f32) -> BlurKernel2d {
    BlurKernel2d::new(radius)
}

/// One post-process effect in a layer's effect chain.
///
/// Effects run in slice order; each one reads the previous result and produces
/// the next image.  See the [module docs](self) for the exact semantics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect2d {
    /// Gaussian blur of the whole layer.
    Blur {
        /// Radius in pixels, clamped to `0.0..=MAX_BLUR_RADIUS`.
        radius: f32,
    },
    /// A tinted, offset, blurred copy of the layer drawn behind it.
    DropShadow {
        /// Straight linear RGBA tint (alpha scales the shadow).
        color: [f32; 4],
        /// Offset in pixels, `[x, y]`, with Y down (screen space).
        offset: [f32; 2],
        /// Blur radius in pixels, clamped to `0.0..=MAX_BLUR_RADIUS`.
        blur: f32,
        /// Extra opacity in `0.0..=1.0`, multiplied into the shadow's alpha.
        opacity: f32,
    },
    /// A tinted, blurred copy of the layer added as light around it (outer
    /// glow: it lights only where the content is not opaque).
    Glow {
        /// Straight linear RGBA tint (alpha scales the glow).
        color: [f32; 4],
        /// Blur radius in pixels, clamped to `0.0..=MAX_BLUR_RADIUS`.
        radius: f32,
        /// Strength of the added light; values above 1 are allowed (HDR).
        intensity: f32,
    },
}

impl Effect2d {
    /// `true` when every parameter is finite.
    ///
    /// Non-finite radii are rejected instead of silently clamped so a document
    /// error surfaces at push time with a typed error.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match *self {
            Self::Blur { radius } => radius.is_finite() && radius >= 0.0,
            Self::DropShadow {
                color,
                offset,
                blur,
                opacity,
            } => {
                color.iter().all(|c| c.is_finite())
                    && offset.iter().all(|o| o.is_finite())
                    && blur.is_finite()
                    && blur >= 0.0
                    && opacity.is_finite()
                    && opacity >= 0.0
            }
            Self::Glow {
                color,
                radius,
                intensity,
            } => {
                color.iter().all(|c| c.is_finite())
                    && radius.is_finite()
                    && radius >= 0.0
                    && intensity.is_finite()
                    && intensity >= 0.0
            }
        }
    }

    /// The blur radius this effect needs, clamped to the kernel limit.
    #[must_use]
    pub fn blur_radius(&self) -> f32 {
        let radius = match *self {
            Self::Blur { radius } | Self::Glow { radius, .. } => radius,
            Self::DropShadow { blur, .. } => blur,
        };
        if radius.is_finite() {
            radius.clamp(0.0, MAX_BLUR_RADIUS)
        } else {
            0.0
        }
    }
}

/// GPU uniform for one blur pass (host-side layout; internal).
///
/// Mirrors `BlurParams` in `effect_blur_2d.wgsl`; `weights` packs
/// [`MAX_BLUR_TAPS`] scalars four per `vec4`.
#[doc(hidden)]
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurUniform2d {
    /// Horizontal `(1, 0)` or vertical `(0, 1)` pass.
    pub direction: [f32; 2],
    /// One-sided tap count, including the centre.
    pub taps: f32,
    /// Effective radius in pixels (diagnostic; the shader uses the weights).
    pub radius: f32,
    /// Kernel weights, four per `vec4`.
    pub weights: [[f32; 4]; MAX_BLUR_TAPS / 4],
}

impl BlurUniform2d {
    /// Uniform for `kernel` in `direction`.
    #[must_use]
    pub fn new(kernel: &BlurKernel2d, direction: [f32; 2]) -> Self {
        let mut weights = [[0.0_f32; 4]; MAX_BLUR_TAPS / 4];
        for (i, weight) in kernel.weights.iter().enumerate() {
            weights[i / 4][i % 4] = *weight;
        }
        Self {
            direction,
            taps: kernel.taps as f32,
            radius: 0.0,
            weights,
        }
    }

    /// Uniform bytes for `queue.write_buffer`.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

/// GPU uniform for one drop-shadow or glow composite pass (internal).
///
/// Mirrors `EffectParams` in `effect_composite_2d.wgsl`.
#[doc(hidden)]
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EffectUniform2d {
    /// Straight linear RGBA tint.
    pub color: [f32; 4],
    /// Blur sample offset in pixels, `[x, y]` (Y down).
    pub offset: [f32; 2],
    /// `0` = drop shadow behind, `1` = glow added.
    pub mode: f32,
    /// Shadow opacity or glow intensity.
    pub strength: f32,
}

impl EffectUniform2d {
    /// Uniform bytes for `queue.write_buffer`.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(kernel: &BlurKernel2d) -> f32 {
        let mut total = kernel.weight(0);
        for i in 1..kernel.taps as usize {
            total += 2.0 * kernel.weight(i);
        }
        total
    }

    #[test]
    fn zero_radius_is_the_identity() {
        let kernel = gaussian_kernel(0.0);
        assert_eq!(kernel.taps, 1);
        assert_eq!(kernel.weight(0), 1.0);
        assert_eq!(kernel.weight(1), 0.0);
    }

    #[test]
    fn negative_and_non_finite_radii_fall_back_to_identity() {
        for radius in [-3.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let kernel = gaussian_kernel(radius);
            assert_eq!(kernel.taps, 1, "radius {radius}");
            assert_eq!(kernel.weight(0), 1.0, "radius {radius}");
        }
    }

    #[test]
    fn kernel_is_normalized_for_every_supported_radius() {
        for radius in [0.5, 1.0, 2.0, 3.5, 8.0, 16.0, 32.0, 48.0, 1000.0] {
            let kernel = gaussian_kernel(radius);
            let total = sum(&kernel);
            assert!(
                (total - 1.0).abs() < 1e-5,
                "radius {radius} sums to {total}"
            );
        }
    }

    #[test]
    fn kernel_is_symmetric_and_monotonically_decreasing() {
        let kernel = gaussian_kernel(12.0);
        assert!(kernel.taps > 2);
        for i in 1..kernel.taps as usize {
            assert!(
                kernel.weight(i) < kernel.weight(i - 1),
                "weight {i} is not below the centre"
            );
        }
        assert!(kernel.weight(0) > kernel.weight(1));
    }

    #[test]
    fn taps_scale_with_the_radius_and_clamp_at_the_limit() {
        assert_eq!(gaussian_kernel(1.0).taps, 3);
        assert_eq!(gaussian_kernel(2.0).taps, 4);
        assert_eq!(gaussian_kernel(10.0).taps, 16);
        assert_eq!(gaussian_kernel(48.0).taps, 73);
        assert!(gaussian_kernel(MAX_BLUR_RADIUS).taps as usize <= MAX_BLUR_TAPS);
        assert_eq!(gaussian_kernel(10_000.0), gaussian_kernel(MAX_BLUR_RADIUS));
    }

    #[test]
    fn weights_round_trip_through_the_public_accessor() {
        let kernel = gaussian_kernel(6.0);
        assert_eq!(kernel.weight(3), kernel.weights[3]);
        assert_eq!(kernel.weight(MAX_BLUR_TAPS + 4), 0.0);
    }

    #[test]
    fn effect_validation_rejects_non_finite_parameters() {
        assert!(Effect2d::Blur { radius: 4.0 }.is_valid());
        assert!(!Effect2d::Blur { radius: f32::NAN }.is_valid());
        assert!(!Effect2d::Blur { radius: -1.0 }.is_valid());
        assert!(!Effect2d::DropShadow {
            color: [1.0, 1.0, 1.0, 1.0],
            offset: [0.0, f32::INFINITY],
            blur: 4.0,
            opacity: 1.0,
        }
        .is_valid());
        assert!(!Effect2d::Glow {
            color: [1.0, 1.0, 1.0, 1.0],
            radius: 4.0,
            intensity: f32::NAN,
        }
        .is_valid());
    }

    #[test]
    fn blur_radius_reads_the_right_field() {
        assert_eq!(
            Effect2d::DropShadow {
                color: [0.0; 4],
                offset: [1.0, 2.0],
                blur: 7.0,
                opacity: 1.0,
            }
            .blur_radius(),
            7.0
        );
        assert_eq!(
            Effect2d::Glow {
                color: [0.0; 4],
                radius: 999.0,
                intensity: 1.0,
            }
            .blur_radius(),
            MAX_BLUR_RADIUS
        );
    }
}
