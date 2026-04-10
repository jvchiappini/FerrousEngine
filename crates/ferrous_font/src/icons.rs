use super::msdf_gen::generate_msdf;
use super::path::GlyphCommand;
use std::collections::HashMap;

/// A simple SVG path parser for MSDF generation.
/// Supports M, L, Q, Z commands.
pub fn parse_svg_path(path_data: &str) -> Vec<GlyphCommand> {
    let mut commands = Vec::new();
    let mut tokens = path_data.split_whitespace();
    let mut start_point = (0.0, 0.0);

    while let Some(token) = tokens.next() {
        match token {
            "M" => {
                let x = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                let y = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                commands.push(GlyphCommand::MoveTo(x, y));
                start_point = (x, y);
            }
            "L" => {
                let x = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                let y = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                commands.push(GlyphCommand::LineTo(x, y));
            }
            "Q" => {
                let cx = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                let cy = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                let tx = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                let ty = tokens.next().and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
                commands.push(GlyphCommand::QuadTo {
                    ctrl_x: cx,
                    ctrl_y: cy,
                    to_x: tx,
                    to_y: ty,
                });
            }
            "Z" => {
                commands.push(GlyphCommand::LineTo(start_point.0, start_point.1));
            }
            _ => {
                // Try parsing as raw numbers (shorthand L)
                if let Ok(x) = token.parse::<f32>() {
                    if let Some(y_token) = tokens.next() {
                        if let Ok(y) = y_token.parse::<f32>() {
                            commands.push(GlyphCommand::LineTo(x, y));
                        }
                    }
                }
            }
        }
    }
    commands
}

#[derive(Debug, Clone)]
pub struct IconMetrics {
    pub uv: [f32; 4],
    pub size: [f32; 2],
}

pub struct IconAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub icons: HashMap<String, IconMetrics>,
}

impl IconAtlas {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        icon_paths: &HashMap<String, String>,
        glyph_size: u32,
    ) -> Self {
        let count = icon_paths.len() as u32;
        let atlas_dim = ((count as f32).sqrt().ceil() as u32).max(1);
        let tex_size = atlas_dim * glyph_size;

        let mut pixels = vec![255u8; (tex_size * tex_size * 4) as usize];
        let mut icons_map = HashMap::new();

        for (i, (name, path_data)) in icon_paths.iter().enumerate() {
            let commands = parse_svg_path(path_data);
            let bmp = generate_msdf(&commands, glyph_size);

            let x = (i as u32 % atlas_dim) * glyph_size;
            let y = (i as u32 / atlas_dim) * glyph_size;

            for row in 0..glyph_size {
                let dst = (((y + row) * tex_size + x) * 4) as usize;
                let src = (row * glyph_size * 4) as usize;
                pixels[dst..dst + (glyph_size * 4) as usize]
                    .copy_from_slice(&bmp[src..src + (glyph_size * 4) as usize]);
            }

            icons_map.insert(
                name.clone(),
                IconMetrics {
                    uv: [
                        x as f32 / tex_size as f32,
                        y as f32 / tex_size as f32,
                        (x + glyph_size) as f32 / tex_size as f32,
                        (y + glyph_size) as f32 / tex_size as f32,
                    ],
                    size: [glyph_size as f32, glyph_size as f32],
                },
            );
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IconAtlas"),
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
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(tex_size * 4), rows_per_image: None },
            wgpu::Extent3d { width: tex_size, height: tex_size, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("IconAtlas Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            icons: icons_map,
        }
    }
}
