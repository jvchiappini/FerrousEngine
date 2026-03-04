use crate::render_target::HdrTexture;
use crate::resources::texture::{self, RenderTextureDesc};

/// Chain of HDR textures used for the bloom downsample/upsample passes.
///
/// Level 0 is half resolution of the screen, level N is repeatedly halved
/// until `levels` textures have been created (or dimension reaches 1).
///
/// The struct retains the underlying `Texture` objects so that the views
/// remain valid; callers primarily interact with `views` when issuing draw
/// commands.  A single `Sampler` is also created and shared by all levels.
///
/// `acc_view` is a separate texture at the same resolution as `views[0]`
/// used as the final accumulation target of the upsample chain.  Keeping it
/// separate from `views[0]` avoids the conflict where `views[0]` holds raw
/// threshold data written by the first downsample pass, which would otherwise
/// corrupt the upsampled bloom result.
pub struct BloomTextures {
    pub textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    /// Accumulation texture: same resolution as views[0], written by the
    /// final upsample pass and sampled by the post-process shader.
    pub acc_texture: wgpu::Texture,
    pub acc_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
}

impl BloomTextures {
    /// Create a new bloom chain for the given full-screen dimensions.
    /// `levels` is the number of mip‑like textures to allocate (4 or 5 is
    /// typical).  The first texture will be half the supplied width/height.
    pub fn new(device: &wgpu::Device, width: u32, height: u32, levels: usize) -> Self {
        let mut textures = Vec::with_capacity(levels);
        let mut views = Vec::with_capacity(levels);

        let mut w = width / 2;
        let mut h = height / 2;
        for i in 0..levels {
            // clamp to at least 1 so we don't create 0-sized textures
            let w_clamped = w.max(1);
            let h_clamped = h.max(1);

            let tex = texture::create_render_texture(
                device,
                &RenderTextureDesc {
                    label: &format!("Bloom mip {}", i),
                    width: w_clamped,
                    height: h_clamped,
                    format: HdrTexture::FORMAT,
                    sample_count: 1,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                },
            );
            let view = texture::default_view(&tex);
            textures.push(tex);
            views.push(view);

            w /= 2;
            h /= 2;
        }

        // Accumulation texture: same size as views[0] (half-screen).
        // This is the final destination of the upsample chain so that it
        // never aliases with the threshold-downsample data in views[0].
        let acc_w = (width / 2).max(1);
        let acc_h = (height / 2).max(1);
        let acc_tex = texture::create_render_texture(
            device,
            &RenderTextureDesc {
                label: "Bloom Accumulation",
                width: acc_w,
                height: acc_h,
                format: HdrTexture::FORMAT,
                sample_count: 1,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        );
        let acc_view = texture::default_view(&acc_tex);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            textures,
            views,
            acc_texture: acc_tex,
            acc_view,
            sampler,
            width,
            height,
        }
    }

    /// Recreate the textures if the size has changed.  No-op otherwise.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        // simply rebuild the whole structure with the same number of levels
        let levels = self.textures.len();
        *self = Self::new(device, width, height, levels);
    }
}
