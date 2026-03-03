use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::resources::{TextureHandle, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE};

/// Simple GPU-backed texture with associated view and sampler.
#[derive(Clone)]
pub struct Texture {
    pub texture: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub sampler: Arc<wgpu::Sampler>,
}

impl Texture {
    /// Create a texture from raw RGBA8 data.  `data` must be `width*height*4`
    /// bytes long.  The resulting texture is immediately uploaded to the GPU.
    /// Create an sRGB texture from raw RGBA8 data (use for albedo / color data).
    /// The GPU will automatically linearize samples from this texture.
    pub fn from_rgba8(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Self {
        Self::from_rgba8_internal(device, queue, width, height, data, true)
    }

    /// Create a **linear** texture from raw RGBA8 data.
    /// Use this for non-color data: normal maps, metallic-roughness, AO, emissive.
    /// The GPU will NOT apply any gamma correction when sampling.
    pub fn from_rgba8_linear(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Self {
        Self::from_rgba8_internal(device, queue, width, height, data, false)
    }

    fn from_rgba8_internal(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
        srgb: bool,
    ) -> Self {
        assert_eq!(data.len(), (width * height * 4) as usize);
        let mip_level_count = (width.max(height) as f32).log2().floor() as u32 + 1;
        let format = if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture::from_rgba8"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            // allocate enough mip levels so sampling at distance works
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                // wgpu versions vary; historically these fields were NonZero,
                // but the compiler errors indicate they are plain u32.  we
                // supply plain values to remain compatible.
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: 0,
            // cover all levels we created
            mip_level_count: None,
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            texture: Arc::new(texture),
            view: Arc::new(view),
            sampler: Arc::new(sampler),
        }
    }
}

/// GPU representation of a material.  Stores a uniform buffer and an
/// associated bind group; the latter can be bound directly during rendering.
#[derive(Clone)]
pub struct Material {
    pub bind_group: Arc<wgpu::BindGroup>,
    /// keep the uniform buffer alive
    pub buffer: Arc<wgpu::Buffer>,
    /// rendering flags required by the renderer
    pub alpha_mode: ferrous_core::scene::AlphaMode,
    pub double_sided: bool,
}

// -----------------------------------------------------------------------------
// material uniform buffer (PBR)
// -----------------------------------------------------------------------------
// the layout is explicitly sized to 96 bytes so that `wgpu` can safely
// declare `min_binding_size` on the bind-group layout.  fields are aligned to
// 16 bytes; after the `flags` field we reserve a full vec4 (16 bytes) so
// that the struct's total size rounds up to a multiple of 16.

// GPU shader expects 16‑byte alignment for every vec4; ensure Rust struct
// has the same alignment so that `bytes_of` yields the correct size.  With
// `align(16)` the struct will round up to 96 bytes automatically.
#[repr(C, align(16))]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniformPbr {
    pub base_color: [f32; 4],         // albedo (linear RGBA)
    pub emissive: [f32; 4],           // xyz=color hdr, w=strength
    pub metallic_roughness: [f32; 4], // x=metallic, y=roughness, z=ao_strength, w=reserved
    pub normal_ao: [f32; 4],          // x=normal_scale, rest reserved
    pub flags: u32,                   // bitmask describing which textures are bound
    // after the flags word we insert a float used for alpha-test cutoff;
    // two u32s pad the remainder so we still hit the 80‑byte boundary.
    pub alpha_cutoff: f32,
    pub _pad: [u32; 2], // fills offset 72..80
    // final padding word to round the total size up to 96 bytes.
    pub _pad1: [u32; 4], // covers 80..96
}

/// bitflags for the `flags` field in [`MaterialUniformPbr`].  these must
/// remain stable because shaders rely on the exact values when sampling
pub const ALBEDO_TEX: u32 = 1 << 0;
pub const NORMAL_TEX: u32 = 1 << 1;
pub const MET_ROUGH_TEX: u32 = 1 << 2;
pub const EMISSIVE_TEX: u32 = 1 << 3;
pub const AO_TEX: u32 = 1 << 4;

// non-texture flag: alpha masking
pub const FLAG_ALPHA_MASK: u32 = 1 << 5;

impl Default for MaterialUniformPbr {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0, 0.0],
            metallic_roughness: [0.0, 0.5, 1.0, 0.0],
            normal_ao: [1.0, 0.0, 0.0, 0.0],
            flags: 0,
            alpha_cutoff: 0.0,
            _pad: [0; 2],
            _pad1: [0; 4],
        }
    }
}

impl Material {
    /// Construct a material from a descriptor.  The caller is responsible
    /// for supplying a texture registry that will be used to resolve any
    /// `TextureHandle` values present in the descriptor.  Missing textures
    /// are automatically replaced with the well-known fallbacks.
    pub fn from_descriptor(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        layouts: &crate::pipeline::PipelineLayouts,
        desc: &ferrous_core::scene::MaterialDescriptor,
        tex_registry: &crate::resources::TextureRegistry,
    ) -> Self {
        // choose sensible fallbacks for any empty slots
        // the descriptor stores raw `u32` indices; convert them to the
        // renderer's `TextureHandle` wrapper before looking up the texture.
        let albedo_handle = TextureHandle(desc.albedo_tex.unwrap_or(TEXTURE_WHITE.0));
        let normal_handle = TextureHandle(desc.normal_tex.unwrap_or(TEXTURE_NORMAL.0));
        // MR fallback: white (metallic=1 in B, roughness=1 in G when no texture) but
        // we default to white=all-ones so metallic/roughness come from the uniform.
        // AO fallback: white = no occlusion (AO=1).
        // These three slots carry linear data; their fallback handles already point
        // to linear textures in the registry (TEXTURE_BLACK / TEXTURE_WHITE are linear).
        let mr_handle = TextureHandle(desc.metallic_roughness_tex.unwrap_or(TEXTURE_WHITE.0));
        let emissive_handle = TextureHandle(desc.emissive_tex.unwrap_or(TEXTURE_BLACK.0));
        let ao_handle = TextureHandle(desc.ao_tex.unwrap_or(TEXTURE_WHITE.0));

        let albedo = tex_registry.get(albedo_handle);
        let normal = tex_registry.get(normal_handle);
        let mr = tex_registry.get(mr_handle);
        let emissive = tex_registry.get(emissive_handle);
        let ao = tex_registry.get(ao_handle);

        // flags for which slots are actually supplied by the user
        let mut flags = 0;
        if desc.albedo_tex.is_some() {
            flags |= ALBEDO_TEX;
        }
        if desc.normal_tex.is_some() {
            flags |= NORMAL_TEX;
        }
        if desc.metallic_roughness_tex.is_some() {
            flags |= MET_ROUGH_TEX;
        }
        if desc.emissive_tex.is_some() {
            flags |= EMISSIVE_TEX;
        }
        if desc.ao_tex.is_some() {
            flags |= AO_TEX;
        }

        // handle alpha mode; mask needs an extra flag and cutoff value
        let mut alpha_cutoff = 0.0;
        if let ferrous_core::scene::AlphaMode::Mask { cutoff } = desc.alpha_mode {
            flags |= FLAG_ALPHA_MASK;
            alpha_cutoff = cutoff;
        }

        let mut uniform = MaterialUniformPbr::default();
        uniform.base_color = desc.base_color;
        uniform.emissive = [
            desc.emissive[0],
            desc.emissive[1],
            desc.emissive[2],
            desc.emissive_strength,
        ];
        uniform.metallic_roughness = [desc.metallic, desc.roughness, desc.ao_strength, 0.0];
        uniform.normal_ao = [desc.normal_scale, 0.0, 0.0, 0.0];
        uniform.flags = flags;
        uniform.alpha_cutoff = alpha_cutoff;

        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("MaterialUniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material BindGroup"),
            layout: &layouts.material,
            entries: &[
                // uniform buffer
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                },
                // sampler (single sampler for all slots)
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                // texture slots
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&albedo.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&normal.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&mr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&emissive.view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&ao.view),
                },
            ],
        });

        Self {
            bind_group: Arc::new(bind_group),
            buffer: Arc::new(buf),
            alpha_mode: desc.alpha_mode.clone(),
            double_sided: desc.double_sided,
        }
    }
}
