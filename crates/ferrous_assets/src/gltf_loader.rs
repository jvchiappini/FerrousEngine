use anyhow::{Context, Result};
use std::path::Path;

/// "Raw" material description produced by the asset loader.  We keep this
/// independent of the renderer's `MaterialDescriptor` type so that the
/// assets crate can remain completely API-agnostic and not pull in any
/// graphics crates or handles.
#[derive(Debug, Clone)]
pub struct RawMaterial {
    pub base_color: [f32; 4],
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    pub ao_strength: f32,
    pub base_color_tex: Option<usize>,
    pub normal_tex: Option<usize>,
    pub metallic_roughness_tex: Option<usize>,
    pub emissive_tex: Option<usize>,
    pub ao_tex: Option<usize>,
    pub alpha_mode: ferrous_core::scene::AlphaMode,
    pub double_sided: bool,
}

/// Mesh data extracted from a GLTF primitive.  Contains separate vertex
/// attribute arrays (positions/normals/uvs/etc.) so that the caller is free
/// to convert them into whatever GPU representation they need.
#[derive(Debug, Clone)]
pub struct AssetMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub material_idx: Option<usize>,
}

/// A complete model loaded from a `.gltf`/`.glb` file.  Images are stored as
/// width/height/raw-pixels (RGBA8).  The ordering of `materials` and
/// `meshes` matches the order of the corresponding objects in the GLTF
/// document; texture indices in the material descriptors refer into the
/// `images` vector.
#[derive(Debug, Clone)]
pub struct AssetModel {
    pub meshes: Vec<AssetMesh>,
    pub materials: Vec<RawMaterial>,
    pub images: Vec<(u32, u32, Vec<u8>)>,
}

/// Load a GLTF/GLB file and return the raw geometry/material data.
///
/// The function is intentionally low‑level; higher layers (core/renderer)
/// will take care of turning this into GPU resources and world entities.
pub fn load_gltf(path: &Path) -> Result<AssetModel> {
    let (document, buffers, images) = gltf::import(path)
        .with_context(|| format!("failed to import glTF '{}'", path.display()))?;

    // --- images -------------------------------------------------------------
    let mut out_images = Vec::with_capacity(images.len());
    use gltf::image::Format as GltfFormat;
    for img in images.iter() {
        // diagnostic: print format/size so we can reason about failures
        eprintln!(
            "gltf image @{}x{} format={:?} bytes={}",
            img.width,
            img.height,
            img.format,
            img.pixels.len(),
        );
        let mut width = img.width;
        let mut height = img.height;
        let mut pixels = match img.format {
            GltfFormat::R8G8B8 => {
                let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
                for chunk in img.pixels.chunks(3) {
                    rgba.extend_from_slice(chunk);
                    rgba.push(255);
                }
                rgba
            }
            GltfFormat::R8G8B8A8 => img.pixels.clone(),
            _ => match image::load_from_memory(&img.pixels) {
                Ok(dyn_img) => {
                    let rgba = dyn_img.to_rgba8();
                    width = rgba.width();
                    height = rgba.height();
                    rgba.into_raw()
                }
                Err(err) => {
                    eprintln!(
                        "warning: failed to decode glTF image ({}x{}) {} bytes: {}",
                        img.width,
                        img.height,
                        img.pixels.len(),
                        err
                    );
                    let mut data = img.pixels.to_vec();
                    if data.len() == (width as usize) * (height as usize) * 3 {
                        let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
                        for chunk in data.chunks(3) {
                            rgba.extend_from_slice(chunk);
                            rgba.push(255);
                        }
                        data = rgba;
                    }
                    let expected = (width as usize) * (height as usize) * 4;
                    if data.len() != expected {
                        if width > 0 && data.len() % ((width as usize) * 4) == 0 {
                            height = (data.len() / ((width as usize) * 4)) as u32;
                        } else if height > 0 && data.len() % ((height as usize) * 4) == 0 {
                            width = (data.len() / ((height as usize) * 4)) as u32;
                        } else {
                            let area = data.len() / 4;
                            let side = (area as f32).sqrt() as u32;
                            if (side as usize) * (side as usize) == area {
                                width = side;
                                height = side;
                            }
                        }
                        eprintln!(
                            "adjusted raw image dims from {}x{} to {}x{}",
                            img.width, img.height, width, height
                        );
                    }
                    data
                }
            },
        };
        // compute simple average colour for debugging
        if !pixels.is_empty() {
            let mut sum = [0u64; 3];
            let mut count = 0u64;
            for chunk in pixels.chunks(4) {
                sum[0] += chunk[0] as u64;
                sum[1] += chunk[1] as u64;
                sum[2] += chunk[2] as u64;
                count += 1;
            }
            if count > 0 {
                eprintln!(
                    " -> avg color = ({:.1},{:.1},{:.1})",
                    sum[0] as f32 / count as f32,
                    sum[1] as f32 / count as f32,
                    sum[2] as f32 / count as f32
                );
            }
        }
        out_images.push((width, height, pixels));
    }
    // --- materials ----------------------------------------------------------
    let mut out_materials = Vec::with_capacity(document.materials().len());
    for mat in document.materials() {
        let pbr = mat.pbr_metallic_roughness();
        let alpha_mode = match mat.alpha_mode() {
            gltf::material::AlphaMode::Opaque => ferrous_core::scene::AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => ferrous_core::scene::AlphaMode::Mask {
                cutoff: mat.alpha_cutoff().unwrap_or(0.5),
            },
            gltf::material::AlphaMode::Blend => ferrous_core::scene::AlphaMode::Blend,
        };
        let raw = RawMaterial {
            base_color: pbr.base_color_factor(),
            emissive: mat.emissive_factor(),
            emissive_strength: mat
                .emissive_factor()
                .iter()
                .copied()
                .fold(0.0, |a, b| a.max(b)),
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            normal_scale: mat.normal_texture().map(|n| n.scale()).unwrap_or(1.0),
            ao_strength: mat.occlusion_texture().map(|o| o.strength()).unwrap_or(1.0),
            base_color_tex: pbr
                .base_color_texture()
                .map(|info| info.texture().source().index()),
            normal_tex: mat
                .normal_texture()
                .map(|info| info.texture().source().index()),
            metallic_roughness_tex: pbr
                .metallic_roughness_texture()
                .map(|info| info.texture().source().index()),
            emissive_tex: mat
                .emissive_texture()
                .map(|info| info.texture().source().index()),
            ao_tex: mat
                .occlusion_texture()
                .map(|info| info.texture().source().index()),
            alpha_mode,
            double_sided: mat.double_sided(),
        };
        out_materials.push(raw);
    }

    // helper to compute tangents if none present
    fn compute_tangents(
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        indices: &[u32],
    ) -> Vec<[f32; 4]> {
        let len = positions.len();
        let mut tan1 = vec![[0.0_f32; 3]; len];
        let mut tan2 = vec![[0.0_f32; 3]; len];
        let mut tangents = vec![[1.0, 0.0, 0.0, 1.0]; len];

        // helper ops
        fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
            [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
        }
        fn mul(a: [f32; 3], s: f32) -> [f32; 3] {
            [a[0] * s, a[1] * s, a[2] * s]
        }
        fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
            [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
        }
        fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
            a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
        }
        fn normalize(v: [f32; 3]) -> [f32; 3] {
            let len_sq = dot(v, v);
            if len_sq > 1e-8 {
                let inv = 1.0 / len_sq.sqrt();
                mul(v, inv)
            } else {
                v
            }
        }

        for idx in indices.chunks(3) {
            if idx.len() < 3 {
                break;
            }
            let i0 = idx[0] as usize;
            let i1 = idx[1] as usize;
            let i2 = idx[2] as usize;
            if i0 >= len || i1 >= len || i2 >= len {
                continue;
            }
            let v0 = positions[i0];
            let v1 = positions[i1];
            let v2 = positions[i2];
            let uv0 = uvs[i0];
            let uv1 = uvs[i1];
            let uv2 = uvs[i2];

            let x1 = v1[0] - v0[0];
            let x2 = v2[0] - v0[0];
            let y1 = v1[1] - v0[1];
            let y2 = v2[1] - v0[1];
            let z1 = v1[2] - v0[2];
            let z2 = v2[2] - v0[2];

            let s1 = uv1[0] - uv0[0];
            let s2 = uv2[0] - uv0[0];
            let t1 = uv1[1] - uv0[1];
            let t2 = uv2[1] - uv0[1];

            let r = 1.0 / (s1 * t2 - s2 * t1);
            let sdir = [
                (t2 * x1 - t1 * x2) * r,
                (t2 * y1 - t1 * y2) * r,
                (t2 * z1 - t1 * z2) * r,
            ];
            let tdir = [
                (s1 * x2 - s2 * x1) * r,
                (s1 * y2 - s2 * y1) * r,
                (s1 * z2 - s2 * z1) * r,
            ];

            tan1[i0] = add(tan1[i0], sdir);
            tan1[i1] = add(tan1[i1], sdir);
            tan1[i2] = add(tan1[i2], sdir);

            tan2[i0] = add(tan2[i0], tdir);
            tan2[i1] = add(tan2[i1], tdir);
            tan2[i2] = add(tan2[i2], tdir);
        }

        for i in 0..len {
            let n = normals[i];
            let t = tan1[i];
            // Gram-Schmidt orthogonalize
            let dot_nt = dot(n, t);
            let mut tangent = sub(t, mul(n, dot_nt));
            tangent = normalize(tangent);
            // handedness
            let cross = [
                n[1] * t[2] - n[2] * t[1],
                n[2] * t[0] - n[0] * t[2],
                n[0] * t[1] - n[1] * t[0],
            ];
            let w = if dot(cross, tan2[i]) < 0.0 { -1.0 } else { 1.0 };
            tangents[i] = [tangent[0], tangent[1], tangent[2], w];
        }

        tangents
    }

    // --- meshes -------------------------------------------------------------
    let mut out_meshes = Vec::new();
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .map(|iter| iter.collect())
                .unwrap_or_default();
            let mut normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|iter| iter.collect())
                .unwrap_or_default();
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|t| t.into_f32().collect())
                .unwrap_or_default();
            if !uvs.is_empty() {
                let (mut min_u, mut max_u) = (uvs[0][0], uvs[0][0]);
                let (mut min_v, mut max_v) = (uvs[0][1], uvs[0][1]);
                for uv in &uvs {
                    min_u = min_u.min(uv[0]);
                    max_u = max_u.max(uv[0]);
                    min_v = min_v.min(uv[1]);
                    max_v = max_v.max(uv[1]);
                }
                eprintln!(
                    "    uv range = u[{:.3},{:.3}] v[{:.3},{:.3}]",
                    min_u, max_u, min_v, max_v
                );
            }
            let colors: Vec<[f32; 3]> = reader
                .read_colors(0)
                .map(|c| c.into_rgba_f32().map(|c| [c[0], c[1], c[2]]).collect())
                .unwrap_or_default();
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|r| r.into_u32().collect())
                .unwrap_or_default();

            let tangents = if let Some(mut t) = reader
                .read_tangents()
                .map(|iter| iter.collect::<Vec<[f32; 4]>>())
            {
                t
            } else {
                compute_tangents(&positions, &normals, &uvs, &indices)
            };

            // ensure we have colour data for every vertex
            let colors = if colors.len() == positions.len() {
                colors
            } else {
                vec![[1.0, 1.0, 1.0]; positions.len()]
            };

            let mesh = AssetMesh {
                positions,
                normals,
                tangents,
                uvs,
                colors,
                indices,
                material_idx: primitive.material().index(),
            };
            if mesh.uvs.is_empty() {
                eprintln!("warning: primitive has no UV coordinates");
            } else {
                eprintln!("first uv = {:?}", mesh.uvs[0]);
            }
            out_meshes.push(mesh);
        }
    }

    Ok(AssetModel {
        meshes: out_meshes,
        materials: out_materials,
        images: out_images,
    })
}
