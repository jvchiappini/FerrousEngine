// for loading from disk when the "image" feature is enabled
#[cfg(feature = "image")]
use image;

// re-exported from material so that callers don't have to know where
// Texture originally lives.  once materials are refactored this import may
// move.
use super::material::Texture;

/// Opaque handle to a texture stored in the [`TextureRegistry`].
/// Internally this is just the index into the registry's vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

/// Well-known constant slots in the registry reserved for fallback
/// textures.  these are always present when a registry is constructed.
pub const TEXTURE_WHITE: TextureHandle = TextureHandle(0);
pub const TEXTURE_NORMAL: TextureHandle = TextureHandle(1);
pub const TEXTURE_BLACK: TextureHandle = TextureHandle(2);

/// Centralized collection of GPU textures.  materials and other subsystems
/// can acquire handles to textures rather than owning individual copies.
/// the registry owns the actual `Texture` instances so they remain alive
/// for the duration of the registry.
#[derive(Clone)]
pub struct TextureRegistry {
    // use `Option` so that slots can be recycled via a free list.  the
    // first three slots (white/normal/black) are special fallbacks and
    // are never removed; all other slots may be set to `None` when the
    // caller frees them.
    textures: Vec<Option<Texture>>,
    /// indices of previously-freed slots that can be reused by future
    /// registrations.  we store the raw u32 because that's what
    /// `TextureHandle` wraps, which makes pushes/pops slightly cheaper.
    free_slots: Vec<u32>,
}

impl TextureRegistry {
    /// Create a fresh registry and populate it with the three mandatory
    /// fallback textures (white, flat normal, black).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut textures = Vec::new();
        let free_slots = Vec::new();

        // white pixel
        textures.push(Some(Texture::from_rgba8(
            device,
            queue,
            1,
            1,
            &[255, 255, 255, 255],
        )));

        // flat normal map (0,0,1) encoded in tangent space
        textures.push(Some(Texture::from_rgba8(
            device,
            queue,
            1,
            1,
            &[127, 127, 255, 255],
        )));

        // black pixel
        textures.push(Some(Texture::from_rgba8(
            device,
            queue,
            1,
            1,
            &[0, 0, 0, 255],
        )));

        Self {
            textures,
            free_slots,
        }
    }

    /// Register raw RGBA8 data as a new texture and return its handle.
    pub fn register_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> TextureHandle {
        // compute mip count even if we don't generate them; the `Texture`
        // helper already does this for us, but we still need to upload each
        // level if we have the image feature.
        #[cfg(feature = "image")]
        {
            // convert raw data into an image for resizing
            let mut current = image::RgbaImage::from_raw(width, height, data.to_vec())
                .expect("register_rgba8 data length mismatch");
            let mip_level_count = (width.max(height) as f32).log2().floor() as u32 + 1;

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Texture::from_rgba8"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // upload base level
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &current,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            // generate the remaining levels using our helper; this also makes
            // hot-reload behave identically.
            generate_mipmaps_cpu(queue, &texture, current, mip_level_count);

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: 0,
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
            let tex = Texture {
                texture: std::sync::Arc::new(texture),
                view: std::sync::Arc::new(view),
                sampler: std::sync::Arc::new(sampler),
            };
            // attempt to reuse a free slot; if none available fall back to
            // growing the vector.  the slot we fill should previously have
            // been set to `None` by `free`.
            if let Some(slot) = self.free_slots.pop() {
                let idx = slot as usize;
                debug_assert!(idx < self.textures.len());
                self.textures[idx] = Some(tex);
                TextureHandle(slot)
            } else {
                let idx = self.textures.len() as u32;
                self.textures.push(Some(tex));
                TextureHandle(idx)
            }
        }
        #[cfg(not(feature = "image"))]
        {
            // fallback: just create a texture with the correct number of mips
            let tex = Texture::from_rgba8(device, queue, width, height, data);
            if let Some(slot) = self.free_slots.pop() {
                let idx = slot as usize;
                self.textures[idx] = Some(tex);
                TextureHandle(slot)
            } else {
                let idx = self.textures.len() as u32;
                self.textures.push(Some(tex));
                TextureHandle(idx)
            }
        }
    }

    /// Load an image from disk and register it.  this helper is only available
    /// when the `image` feature is enabled.
    #[cfg(feature = "image")]
    pub fn register_from_path<P: AsRef<std::path::Path>>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
    ) -> Result<TextureHandle, image::ImageError> {
        // load the source image
        let mut current_img = image::open(path)?.into_rgba8();
        let (width, height) = current_img.dimensions();

        // compute mip level count (at least 1)
        let mip_level_count = (width.max(height) as f32).log2().floor() as u32 + 1;

        // allocate the texture with all mip levels
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("TextureRegistry::from_path"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // upload base level
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            current_img.as_raw(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // generate and upload mipmaps on the CPU side
        generate_mipmaps_cpu(queue, &texture, current_img, mip_level_count);

        // create a Texture wrapper identical to Texture::from_rgba8 but
        // matching the descriptor we just built.  we intentionally duplicate
        // code instead of calling `from_rgba8` because that helper assumes a
        // single mip level and performs its own upload.
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: 0,
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

        let tex = Texture {
            texture: std::sync::Arc::new(texture),
            view: std::sync::Arc::new(view),
            sampler: std::sync::Arc::new(sampler),
        };
        if let Some(slot) = self.free_slots.pop() {
            let idx = slot as usize;
            self.textures[idx] = Some(tex);
            Ok(TextureHandle(slot))
        } else {
            let idx = self.textures.len() as u32;
            self.textures.push(Some(tex));
            Ok(TextureHandle(idx))
        }
    }

    /// Access a texture by handle.  panics if the handle is out of range.
    pub fn get(&self, handle: TextureHandle) -> &Texture {
        // protect against out‑of‑bounds handles or slots that have been
        // freed.  in either case we return the white fallback texture so
        // that rendering continues rather than panicking/crashing.
        let idx = handle.0 as usize;
        if idx < self.textures.len() {
            if let Some(ref tex) = self.textures[idx] {
                return tex;
            }
        }
        // fall back to white pixel (slot 0 is guaranteed to exist and be
        // `Some` because we never free the builtins).
        self.textures[0].as_ref().unwrap()
    }

    /// Number of textures currently registered (including the three
    /// fallbacks).
    pub fn len(&self) -> usize {
        self.textures.len()
    }
}

impl TextureRegistry {
    /// Free a texture previously created with [`register_rgba8`] or
    /// [`register_from_path`].  Freed slots are placed on the free list and
    /// may be reused by future registrations.  It is safe to call this
    /// multiple times for the same handle; subsequent calls are ignored.
    pub fn free(&mut self, handle: TextureHandle) {
        // never delete the fallback textures.
        if handle.0 <= TEXTURE_BLACK.0 {
            return;
        }
        let idx = handle.0 as usize;
        if idx < self.textures.len() {
            if self.textures[idx].is_some() {
                self.textures[idx] = None;
                self.free_slots.push(handle.0);
            }
        }
    }

    /// Update the pixel data for an existing texture.  This is used for
    /// hot-reloading assets without recreating bind groups; the underlying
    /// `wgpu::Texture` object remains the same so any material referencing
    /// the handle will immediately see the new pixels on the next draw.
    pub fn update_texture_data(
        &mut self,
        queue: &wgpu::Queue,
        handle: TextureHandle,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        let idx = handle.0 as usize;
        if idx >= self.textures.len() {
            return;
        }
        let tex_opt = &self.textures[idx];
        if let Some(ref tex) = tex_opt {
            // write base level
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &*tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );

            // optionally regenerate mipmaps on the CPU when `image`
            // feature is enabled.  this mirrors the behaviour from
            // registration helpers so that hot‑reload looks identical to a
            // fresh texture.
            #[cfg(feature = "image")]
            {
                // reuse the same helper used during creation
                let mut current = image::RgbaImage::from_raw(width, height, data.to_vec())
                    .expect("update_texture_data length mismatch");
                let mip_level_count = (width.max(height) as f32).log2().floor() as u32 + 1;
                generate_mipmaps_cpu(queue, &*tex.texture, current, mip_level_count);
            }
        }
    }
}

// helper shared by registration and hot-reload paths
#[cfg(feature = "image")]
fn generate_mipmaps_cpu(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mut current: image::RgbaImage,
    mip_level_count: u32,
) {
    let (width, height) = current.dimensions();
    for mip in 1..mip_level_count {
        let mip_w = (width >> mip).max(1);
        let mip_h = (height >> mip).max(1);
        let resized = image::imageops::resize(
            &current,
            mip_w,
            mip_h,
            image::imageops::FilterType::Lanczos3,
        );

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            resized.as_raw(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * mip_w),
                rows_per_image: Some(mip_h),
            },
            wgpu::Extent3d {
                width: mip_w,
                height: mip_h,
                depth_or_array_layers: 1,
            },
        );
        current = resized;
    }
}
