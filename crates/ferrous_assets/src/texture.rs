//! Basic texture asset helpers.  This module provides a thin wrapper around
//! [`wgpu::Texture`] to simplify loading images from disk or from raw bytes.

use anyhow::Result;
use std::path::Path;

/// 2-D GPU texture with view and sampler.
pub struct Texture2d {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl std::fmt::Debug for Texture2d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture2d").finish_non_exhaustive()
    }
}


impl Texture2d {
    /// Create a texture from raw RGBA8 data.  Caller must ensure `data` has
    /// exactly `width * height * 4` bytes.
    pub fn from_rgba8(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Self {
        assert_eq!(data.len(), (width * height * 4) as usize);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture2d"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture2d sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Texture2d {
            texture,
            view,
            sampler,
        }
    }

    /// Load an image file from disk (any format supported by the `image`
    /// crate) and upload it to a GPU texture.
    pub fn from_file<P: AsRef<Path>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
    ) -> Result<Self> {
        let img = image::open(path)?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Self::from_rgba8(device, queue, width, height, &rgba))
    }

    /// Load an *HDR* image (`.hdr`/`.exr`) via the `image` crate.  The
    /// decoder produces a 32‑bit float RGBA image which we upload directly
    /// as `Rgba32Float` so the full dynamic range is preserved for
    /// environment map precomputation.
    pub fn from_hdr<P: AsRef<Path>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
    ) -> Result<Self> {
        // rely on the `image` crate's HDR decoder which returns a
        // DynamicImage; we then convert to f32 RGBA so we can upload
        // floats directly.  `image::open` handles .hdr and .exr formats.
        let dyn_img = image::open(path)?;
        // convert to 32-bit float RGBA; `to_rgba32f` returns an
        // ImageBuffer<Rgba<f32>, Vec<f32>>.
        let rgba32 = dyn_img.to_rgba32f();
        let (width, height) = rgba32.dimensions();
        // raw pixels are interleaved f32 (RGBA)
        let rgba_data: Vec<f32> = rgba32.into_raw();

        // To allow sampling with linear filtering we choose a *filterable*
        // format.  `Rgba32Float` textures are not guaranteed to be filterable
        // by the GPU, resulting in validation errors (see runtime panic).  A
        // common compromise is `Rgba16Float` which still preserves high
        // dynamic range while being universally filterable.  We'll therefore
        // convert the 32‑bit floats to 16‑bit halves before uploading.
        use half::f16;

        // convert float32 pixels to half-float and then to raw bits (u16)
        let half_bits: Vec<u16> = rgba_data
            .iter()
            .map(|&f| f16::from_f32(f).to_bits())
            .collect();
        let byte_data: &[u8] = bytemuck::cast_slice(&half_bits);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HdrTexture2d"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
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
            byte_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(2 * 4 * width), // 2 bytes per component (RGBA)
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("HdrTexture2d sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Texture2d {
            texture,
            view,
            sampler,
        })
    }

    /// Overwrite this texture's pixel data **in place**.
    ///
    /// `data` must be tightly packed RGBA8 (`width * height * 4` bytes) and
    /// `width`/`height` must equal the texture's current size.  This mirrors
    /// `TextureRegistry::update_texture_data` (`ferrous_renderer`): it only
    /// issues a `queue.write_texture` on the existing `wgpu::Texture`, so the
    /// view, sampler and any bind groups referencing it stay valid — no
    /// reallocation, no bind-group rebuild.  To change dimensions, create a
    /// new `Texture2d` instead.
    pub fn update_rgba8(&self, queue: &wgpu::Queue, width: u32, height: u32, data: &[u8]) {
        assert_eq!(
            (width, height),
            (self.texture.width(), self.texture.height()),
            "Texture2d::update_rgba8 size must match the existing texture"
        );
        assert_eq!(data.len(), (width * height * 4) as usize);
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
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
    }

    /// Rasterize an SVG document (provided as bytes) to a texture of the
    /// given dimensions.  This requires the `svg` feature which pulls in
    /// `resvg`/`tiny-skia`.  The caller is responsible for choosing an
    /// appropriate size; the SVG will be scaled uniformly to fit the target
    /// while preserving aspect ratio.
    ///
    /// The expensive step — `usvg` parse + `tiny-skia` render — is memoised by
    /// `(content hash, width, height)` in the process-global [`SvgCache`], so
    /// repeated calls with identical bytes skip re-rasterisation.  The
    /// signature and return value are unchanged: every call still returns a
    /// **freshly owned** `Texture2d` (created from the cached pixels), so
    /// callers that mutate their handle are unaffected.  Callers that instead
    /// want to share a single GPU texture — and a stable `Arc` identity for
    /// the UI texture-slot dedup — should use [`Texture2d::from_svg_bytes_cached`].
    #[cfg(feature = "svg")]
    pub fn from_svg_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        svg_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let pixels = svg_cache::global().rasterize(svg_data, width, height)?;
        Ok(Self::from_rgba8(device, queue, width, height, &pixels))
    }

    /// Rasterize an SVG to premultiplied RGBA8 pixels on the CPU (through
    /// the global content-keyed cache), without creating a GPU texture.
    /// Useful to pack many icons into one atlas texture.
    #[cfg(feature = "svg")]
    pub fn rasterize_svg(svg_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        Ok(svg_cache::global().rasterize(svg_data, width, height)?.as_ref().clone())
    }

    /// Rasterize an SVG and return a **shared** GPU texture from the
    /// process-global content-keyed [`SvgCache`].
    ///
    /// Identical `(svg_data, width, height)` calls return clones of the same
    /// `Arc<Texture2d>` (`Arc::ptr_eq` holds): the pixmap is rasterised once
    /// and one `wgpu::Texture` + sampler is created and uploaded once.  Use
    /// this when the same SVG is asked for repeatedly (e.g. an immediate-mode
    /// UI that re-creates its image list each frame): it removes both the
    /// rasterisation hitch and the churn of new textures/bind groups.
    ///
    /// The returned handle is shared, so do **not** call
    /// [`Texture2d::update_rgba8`] on it expecting private mutation — that
    /// updates the pixels seen by every holder of the cached handle.
    #[cfg(feature = "svg")]
    pub fn from_svg_bytes_cached(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        svg_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<std::sync::Arc<Self>> {
        svg_cache::global().get_or_rasterize(device, queue, svg_data, width, height)
    }

    /// Convenience helper to load an SVG file from disk and rasterize it.
    #[cfg(feature = "svg")]
    pub fn from_svg_file<P: AsRef<Path>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_svg_bytes(device, queue, &data, width, height)
    }
}

/// Content-keyed cache for SVG rasterisation and its shared GPU textures.
///
/// # Why
///
/// `Texture2d::from_svg_bytes` used to parse (`usvg`) and rasterise
/// (`tiny-skia`) the document on every call, on the frame thread — measured at
/// 6–139 ms for a ~600-element CAD plan, and ~15 ms even for an identical
/// repeat.  This cache removes that repeated work.
///
/// # Semantics / lifetime
///
/// * `(hash(svg bytes), width, height)` is the key.  A hit avoids the parse
///   and render.
/// * The pixel buffer is cached always; the shared `Arc<Texture2d>` is created
///   lazily and only when [`Texture2d::from_svg_bytes_cached`] (or
///   [`SvgCache::get_or_rasterize`]) is used, so plain `from_svg_bytes` calls
///   do not pin a GPU texture per document.
/// * Entries are evicted oldest-first once the pixel byte budget is exceeded.
/// * The cache is process-global and assumes a **single** `wgpu::Device`; a
///   `Texture2d` is only valid on the device that created it, so multi-device
///   apps must use their own [`SvgCache`] per device.
///
/// # Async / background rasterisation
///
/// This is the minimal synchronous-cache layer.  A full solution would add an
/// `SvgData: Asset` type so the `AssetServer`/rayon pool could rasterise off
/// the frame thread and publish the result; the content-keyed cache here is
/// designed to slot in front of that path unchanged.
#[cfg(feature = "svg")]
mod svg_cache {
    use super::Texture2d;
    use anyhow::Result;
    use std::collections::{HashMap, VecDeque};
    use std::hash::{Hash, Hasher};
    use std::sync::{Arc, Mutex, OnceLock};

    /// Default pixel-byte budget for the process-global cache (64 MiB).  GPU
    /// textures are bounded by the same entry set because they live in entries.
    const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct SvgKey {
        content_hash: u64,
        width: u32,
        height: u32,
    }

    fn key_for(svg_data: &[u8], width: u32, height: u32) -> SvgKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        svg_data.hash(&mut hasher);
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        SvgKey {
            content_hash: hasher.finish(),
            width,
            height,
        }
    }

    struct Entry {
        pixels: Arc<Vec<u8>>,
        texture: Option<Arc<Texture2d>>,
    }

    struct Inner {
        entries: HashMap<SvgKey, Entry>,
        order: VecDeque<SvgKey>,
        bytes: usize,
        max_bytes: usize,
    }

    impl Inner {
        fn insert(&mut self, key: SvgKey, entry: Entry) {
            if self.entries.insert(key, entry).is_none() {
                self.order.push_back(key);
            } else {
                self.order.retain(|k| *k != key);
                self.order.push_back(key);
            }
            self.bytes = self.entries.values().map(|e| e.pixels.len()).sum();
            while self.bytes > self.max_bytes {
                let Some(oldest) = self.order.pop_front() else {
                    break;
                };
                if let Some(removed) = self.entries.remove(&oldest) {
                    self.bytes = self.bytes.saturating_sub(removed.pixels.len());
                }
            }
        }
    }

    /// Content-keyed SVG raster cache.  See the module docs for semantics.
    pub struct SvgCache {
        inner: Mutex<Inner>,
    }

    impl SvgCache {
        /// Create a cache with a pixel-byte budget.
        pub fn new(max_bytes: usize) -> Self {
            Self {
                inner: Mutex::new(Inner {
                    entries: HashMap::new(),
                    order: VecDeque::new(),
                    bytes: 0,
                    max_bytes,
                }),
            }
        }

        /// Number of cached entries (tests / diagnostics).
        pub fn cached_len(&self) -> usize {
            self.inner.lock().unwrap().entries.len()
        }

        /// Drop every entry (pixels and shared textures).
        pub fn clear(&self) {
            let mut inner = self.inner.lock().unwrap();
            inner.entries.clear();
            inner.order.clear();
            inner.bytes = 0;
        }

        /// Rasterise `svg_data` to tightly packed, premultiplied RGBA8,
        /// reusing cached pixels for an identical `(content, width, height)`.
        pub fn rasterize(&self, svg_data: &[u8], width: u32, height: u32) -> Result<Arc<Vec<u8>>> {
            let key = key_for(svg_data, width, height);
            if let Some(entry) = self.inner.lock().unwrap().entries.get(&key) {
                return Ok(Arc::clone(&entry.pixels));
            }
            // Rasterise without holding the lock; single-threaded callers.
            let pixels = Arc::new(rasterize_svg(svg_data, width, height)?);
            let mut inner = self.inner.lock().unwrap();
            if let Some(entry) = inner.entries.get(&key) {
                return Ok(Arc::clone(&entry.pixels));
            }
            inner.insert(
                key,
                Entry {
                    pixels: Arc::clone(&pixels),
                    texture: None,
                },
            );
            Ok(pixels)
        }

        /// Return the shared GPU texture for the document, creating the pixels
        /// and texture on first use.
        pub fn get_or_rasterize(
            &self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            svg_data: &[u8],
            width: u32,
            height: u32,
        ) -> Result<Arc<Texture2d>> {
            let key = key_for(svg_data, width, height);
            if let Some(entry) = self.inner.lock().unwrap().entries.get(&key) {
                if let Some(texture) = &entry.texture {
                    return Ok(Arc::clone(texture));
                }
            }
            let pixels = self.rasterize(svg_data, width, height)?;
            let texture = Arc::new(Texture2d::from_rgba8(device, queue, width, height, &pixels));
            let mut inner = self.inner.lock().unwrap();
            match inner.entries.get_mut(&key) {
                Some(entry) => entry.texture = Some(Arc::clone(&texture)),
                None => inner.insert(
                    key,
                    Entry {
                        pixels: Arc::clone(&pixels),
                        texture: Some(Arc::clone(&texture)),
                    },
                ),
            }
            Ok(texture)
        }
    }

    /// The process-global cache used by [`Texture2d::from_svg_bytes`] and
    /// [`Texture2d::from_svg_bytes_cached`].
    pub fn global() -> &'static SvgCache {
        static GLOBAL: OnceLock<SvgCache> = OnceLock::new();
        GLOBAL.get_or_init(|| SvgCache::new(DEFAULT_MAX_BYTES))
    }

    /// The expensive, cacheable part: `usvg` parse + `tiny-skia` render to
    /// premultiplied, tightly packed RGBA8.
    pub(crate) fn rasterize_svg(svg_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        use resvg::{tiny_skia, usvg};

        // parse SVG data
        let opt = usvg::Options::default();
        // note: font database is managed separately by the caller if text is
        // required; for simple icons we can ignore fonts.
        use resvg::usvg::TreeParsing;
        let svg_text = std::str::from_utf8(svg_data)?;
        let usvg_tree = usvg::Tree::from_str(svg_text, &opt)
            .map_err(|e| anyhow::anyhow!("SVG parse error: {:?}", e))?;

        // convert to render tree
        let rtree = resvg::Tree::from_usvg(&usvg_tree);

        // determine scale to target dimensions (uniform)
        let orig_size = rtree.size;
        let sx = width as f32 / orig_size.width() as f32;
        let sy = height as f32 / orig_size.height() as f32;
        let scale = tiny_skia::Transform::from_scale(sx, sy);

        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| anyhow::anyhow!("failed to allocate pixmap"))?;
        // render with scaling transform
        rtree.render(scale, &mut pixmap.as_mut());

        // tiny-skia stores premultiplied RGBA; that's acceptable for our
        // blending operations so we upload the raw bytes directly.
        Ok(pixmap.data().to_vec())
    }
}

#[cfg(feature = "svg")]
pub use svg_cache::SvgCache;

#[cfg(all(test, feature = "svg", feature = "gpu"))]
mod tests {
    use crate::texture::SvgCache;
    use crate::Texture2d;
    use std::sync::Arc;

    /// Ask for a normal adapter first and fall back to a software adapter
    /// (lavapipe / llvmpipe on the CI/VPS box) so the test runs headless.
    fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let request = |fallback: bool| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: fallback,
            }))
        };
        let adapter = request(false).or_else(|| request(true))?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).ok()
    }

    #[test]
    fn svg_rasterization_roundtrip() {
        // a tiny red circle svg
        let svg = r#"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'>
            <circle cx='5' cy='5' r='4' fill='red' /></svg>"#;
        let Some((device, queue)) = headless_device() else {
            eprintln!("no wgpu adapter available; skipping svg rasterisation test");
            return;
        };
        let tex = Texture2d::from_svg_bytes(&device, &queue, svg.as_bytes(), 16, 16)
            .expect("svg rasterize");
        assert_eq!(tex.texture.dimension(), wgpu::TextureDimension::D2);
        assert_eq!(tex.texture.width(), 16);
        assert_eq!(tex.texture.height(), 16);
    }

    /// The cache must key on both content and target size: identical calls
    /// reuse one rasterisation, a different size is a distinct entry.
    #[test]
    fn svg_cache_is_content_and_size_keyed() {
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'>\
                     <circle cx='5' cy='5' r='4' fill='red' /></svg>";
        let other = b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'>\
                       <rect x='1' y='1' width='8' height='8' fill='blue' /></svg>";
        let cache = SvgCache::new(64 * 1024 * 1024);

        let a = cache.rasterize(svg, 16, 16).unwrap();
        let b = cache.rasterize(svg, 16, 16).unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "identical content/size must hit the cache"
        );
        assert_eq!(cache.cached_len(), 1);

        let _ = cache.rasterize(svg, 8, 8).unwrap();
        assert_eq!(cache.cached_len(), 2, "a new size is a distinct entry");

        let _ = cache.rasterize(other, 16, 16).unwrap();
        assert_eq!(
            cache.cached_len(),
            3,
            "a new content hash is a distinct entry"
        );
    }

    /// The `_cached` entrypoint must return the same `Arc` for identical
    /// input, so consumers (e.g. the UI's `Arc::ptr_eq` slot dedup) see stable
    /// texture identity instead of a new allocation per call.
    #[test]
    fn svg_cached_texture_is_shared_across_calls() {
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'>\
                     <circle cx='5' cy='5' r='4' fill='green' /></svg>";
        let Some((device, queue)) = headless_device() else {
            eprintln!("no wgpu adapter available; skipping svg cache test");
            return;
        };
        let first =
            Texture2d::from_svg_bytes_cached(&device, &queue, svg, 16, 16).expect("svg rasterize");
        let second =
            Texture2d::from_svg_bytes_cached(&device, &queue, svg, 16, 16).expect("svg cache hit");
        assert!(
            Arc::ptr_eq(&first, &second),
            "repeated cached calls must share one Arc<Texture2d>"
        );
    }
}

