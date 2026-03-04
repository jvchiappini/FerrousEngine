/// CPU-side SSAO resources: hemisphere kernel + 4×4 noise texture.
///
/// The kernel contains 64 `vec3` samples distributed in a hemisphere
/// oriented toward +Z (tangent space).  Samples are biased toward the
/// origin using a lerp so that close-range occlusion is weighted more
/// heavily.
///
/// The noise texture stores 16 random rotation vectors (XY plane) that
/// tile across the screen at 4-pixel intervals.  Tiling lets us rotate
/// the kernel per pixel cheaply while re-using the same 16 vectors.
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rand::Rng;
use wgpu::util::DeviceExt;

// ── GPU-facing structs ────────────────────────────────────────────────────────

/// A single kernel sample packed as vec4 for 16-byte alignment.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct KernelSample {
    /// xyz = hemisphere direction (view space, oriented toward +Z), w = unused.
    pub direction: [f32; 4],
}

/// The 64-sample kernel uploaded as a uniform buffer.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct SsaoKernelUniform {
    pub samples: [KernelSample; 64],
}

/// Per-frame parameters uploaded every resize / parameter change.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct SsaoParams {
    /// `(ssao_w / 4, ssao_h / 4)` — the tiling factor for the 4-pixel noise.
    pub noise_scale: [f32; 2],
    /// World-space hemisphere radius (editor-exposed).
    pub radius: f32,
    /// Self-occlusion bias.
    pub bias: f32,
    /// Projection matrix (col-major).
    pub proj: [[f32; 4]; 4],
    /// Inverse projection matrix (col-major).
    pub inv_proj: [[f32; 4]; 4],
    /// SSAO texture dimensions (half-res).
    pub screen_size: [f32; 2],
    /// Number of active kernel samples (≤ 64).
    pub kernel_size: u32,
    pub _pad: u32,
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct SsaoResources {
    /// 64-sample hemisphere kernel (uniform buffer).
    pub kernel_buffer: wgpu::Buffer,
    /// SSAO params (uniform buffer, updated every frame/resize).
    pub params_buffer: wgpu::Buffer,
    /// 4×4 RGBA8 noise texture.
    pub noise_texture: wgpu::Texture,
    pub noise_view: wgpu::TextureView,
    pub noise_sampler: wgpu::Sampler,
    /// Cached kernel data so callers can read it without going to the GPU.
    pub kernel: SsaoKernelUniform,
    /// Current SSAO radius (editor-visible).
    pub radius: f32,
    /// Bias for depth comparison.
    pub bias: f32,
    /// Number of kernel samples (1–64).
    pub kernel_size: u32,
}

impl SsaoResources {
    /// Default SSAO radius (view-space units, roughly 0.5 m).
    pub const DEFAULT_RADIUS: f32 = 0.5;
    /// Default self-occlusion bias.
    pub const DEFAULT_BIAS: f32 = 0.025;
    /// Number of hemisphere samples.
    pub const KERNEL_SIZE: u32 = 64;

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let kernel = Self::generate_kernel();
        let noise_data = Self::generate_noise();

        // ── Kernel buffer ─────────────────────────────────────────────────────
        let kernel_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSAO Kernel Buffer"),
            contents: bytemuck::bytes_of(&kernel),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Params buffer (zeroed; first frame will upload real values) ────────
        let params_zero = SsaoParams {
            noise_scale: [1.0, 1.0],
            radius: Self::DEFAULT_RADIUS,
            bias: Self::DEFAULT_BIAS,
            proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            inv_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            screen_size: [1.0, 1.0],
            kernel_size: Self::KERNEL_SIZE,
            _pad: 0,
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SSAO Params Buffer"),
            contents: bytemuck::bytes_of(&params_zero),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── 4×4 noise texture ─────────────────────────────────────────────────
        let noise_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSAO Noise Texture"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Rgba8Unorm: each channel stores a value in [0, 1].
            // We pack the XYZ rotation vector as (r, g, b) and leave a = 1.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            noise_texture.as_image_copy(),
            &noise_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * 4), // 4 pixels × 4 bytes (RGBA)
                rows_per_image: Some(4),
            },
            wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        );

        let noise_view = noise_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Wrap + nearest filtering: tiles perfectly and avoids interpolation
        // between noise vectors (which would create artefacts).
        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSAO Noise Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            kernel_buffer,
            params_buffer,
            noise_texture,
            noise_view,
            noise_sampler,
            kernel,
            radius: Self::DEFAULT_RADIUS,
            bias: Self::DEFAULT_BIAS,
            kernel_size: Self::KERNEL_SIZE,
        }
    }

    /// Upload updated `SsaoParams` to the GPU.
    pub fn update_params(
        &self,
        queue: &wgpu::Queue,
        ssao_width: u32,
        ssao_height: u32,
        proj: glam::Mat4,
        inv_proj: glam::Mat4,
    ) {
        let params = SsaoParams {
            noise_scale: [ssao_width as f32 / 4.0, ssao_height as f32 / 4.0],
            radius: self.radius,
            bias: self.bias,
            proj: proj.to_cols_array_2d(),
            inv_proj: inv_proj.to_cols_array_2d(),
            screen_size: [ssao_width as f32, ssao_height as f32],
            kernel_size: self.kernel_size.min(64),
            _pad: 0,
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
    }

    // ── Private generators ────────────────────────────────────────────────────

    /// Generate 64 hemisphere samples biased toward the origin.
    fn generate_kernel() -> SsaoKernelUniform {
        let mut rng = rand::thread_rng();
        let mut samples = [KernelSample {
            direction: [0.0; 4],
        }; 64];

        for (i, sample) in samples.iter_mut().enumerate() {
            // Random direction in the +Z hemisphere
            let x = rng.gen_range(-1.0_f32..1.0_f32);
            let y = rng.gen_range(-1.0_f32..1.0_f32);
            let z = rng.gen_range(0.0_f32..1.0_f32);
            let mut s = Vec3::new(x, y, z).normalize();

            // Random magnitude
            let scale_t = i as f32 / 64.0;
            // Accelerating interpolation: more samples cluster near the origin
            let scale = lerp(0.1, 1.0, scale_t * scale_t);
            s *= rng.gen_range(0.0_f32..1.0_f32) * scale;

            sample.direction = [s.x, s.y, s.z, 0.0];
        }

        SsaoKernelUniform { samples }
    }

    /// Generate a 4×4 noise texture with random XY rotation vectors packed as RGBA8.
    fn generate_noise() -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let mut data = Vec::with_capacity(4 * 4 * 4); // 16 pixels × 4 bytes

        for _ in 0..16 {
            // Random vector in XY plane (z = 0), normalised
            let x = rng.gen_range(-1.0_f32..1.0_f32);
            let y = rng.gen_range(-1.0_f32..1.0_f32);
            let v = Vec3::new(x, y, 0.0).normalize();

            // Pack [-1, 1] → [0, 255]
            let r = ((v.x * 0.5 + 0.5) * 255.0) as u8;
            let g = ((v.y * 0.5 + 0.5) * 255.0) as u8;
            let b = 128_u8; // z = 0 packed
            let a = 255_u8;
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(a);
        }

        data
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
