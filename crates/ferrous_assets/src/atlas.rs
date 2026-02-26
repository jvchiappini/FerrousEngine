//! Atlas packing and GPU texture management for glyph MSDFs.

use anyhow::Result;
use std::collections::HashMap;
use crate::msdf_gen::generate_msdf;

#[derive(Debug, Clone)]
pub struct GlyphMetrics {
    pub uv: [f32; 4],
    pub size: [f32; 2],
    pub advance: f32,
}

pub struct FontAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub metrics: HashMap<char, GlyphMetrics>,
    pub glyph_size: u32,
}

impl FontAtlas {
    pub fn new<I: IntoIterator<Item = char>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        parser: &crate::font_parser::FontParser,
        chars: I,
    ) -> Result<Self> {
        let glyph_size = 64;
        let char_list: Vec<char> = chars.into_iter().collect();
        let count = char_list.len() as u32;
        if count == 0 { return Err(anyhow::anyhow!("no characters")); }
        
        let atlas_dim = ((count as f32).sqrt().ceil() as u32).max(1);
        let tex_size = atlas_dim * glyph_size;
        println!("Building atlas {}x{} for {} glyphs", tex_size, tex_size, count);
        
        let mut pixels = vec![255u8; (tex_size * tex_size * 4) as usize];
        let mut metrics = HashMap::new();

        let mut x = 0;
        let mut y = 0;
        for &c in &char_list {
            let outline = parser.get_glyph_outline(c);
            let advance = parser.get_glyph_advance(c);
            let bmp = generate_msdf(&outline, glyph_size);
            
            for row in 0..glyph_size {
                let dst = (((y + row) * tex_size + x) * 4) as usize;
                let src = (row * glyph_size * 4) as usize;
                pixels[dst..dst + (glyph_size * 4) as usize].copy_from_slice(&bmp[src..src + (glyph_size * 4) as usize]);
            }

            metrics.insert(c, GlyphMetrics {
                uv: [
                    x as f32 / tex_size as f32,
                    y as f32 / tex_size as f32,
                    (x + glyph_size) as f32 / tex_size as f32,
                    (y + glyph_size) as f32 / tex_size as f32,
                ],
                size: [glyph_size as f32, glyph_size as f32],
                advance,
            });

            x += glyph_size;
            if x + glyph_size > tex_size { x = 0; y += glyph_size; }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("FontAtlas"),
            size: wgpu::Extent3d { width: tex_size, height: tex_size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
             wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &pixels,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some((tex_size * 4).try_into().unwrap()), rows_per_image: None },
            wgpu::Extent3d { width: tex_size, height: tex_size, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("FontAtlas Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });


        Ok(FontAtlas { texture, view, sampler, metrics, glyph_size })
    }
}
