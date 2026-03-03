use anyhow::Result;
use std::path::Path;
use crate::Vec3;

/// Helper that loads a GLTF/GLB file via `ferrous_assets`, registers the
/// resulting textures, materials and meshes with the renderer, and spawns an
/// entity for each mesh in the provided world.  The returned vector contains
/// the handles of the newly-created entities, in the same order as the
/// meshes stored inside the file.
pub fn spawn_gltf(
    world: &mut ferrous_core::scene::World,
    renderer: &mut ferrous_renderer::Renderer,
    path: &str,
) -> Result<Vec<ferrous_core::scene::Handle>> {
    let path = Path::new(path);
    let model = ferrous_assets::load_gltf(path)?;

    // register images first so we can refer to their slots when building
    // materials.
    let mut tex_handles = Vec::with_capacity(model.images.len());
    for (w, h, pixels) in &model.images {
        let th = renderer.register_texture(*w, *h, pixels);
        tex_handles.push(th);
    }

    // convert raw materials into engine descriptors and register them.
    let mut mat_handles = Vec::with_capacity(model.materials.len());
    for raw in &model.materials {
        let mut desc = ferrous_core::scene::MaterialDescriptor::default();
        desc.base_color = raw.base_color;
        desc.emissive = raw.emissive;
        desc.emissive_strength = raw.emissive_strength;
        desc.metallic = raw.metallic;
        desc.roughness = raw.roughness;
        desc.normal_scale = raw.normal_scale;
        desc.ao_strength = raw.ao_strength;
        desc.alpha_mode = raw.alpha_mode.clone();
        desc.double_sided = raw.double_sided;

        if let Some(idx) = raw.base_color_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.albedo_tex = Some(tex.0);
            }
        }
        if let Some(idx) = raw.normal_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.normal_tex = Some(tex.0);
            }
        }
        if let Some(idx) = raw.metallic_roughness_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.metallic_roughness_tex = Some(tex.0);
            }
        }
        if let Some(idx) = raw.emissive_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.emissive_tex = Some(tex.0);
            }
        }
        if let Some(idx) = raw.ao_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.ao_tex = Some(tex.0);
            }
        }

        let mh = renderer.create_material(&desc);
        mat_handles.push(mh);
    }

    let mut out_handles = Vec::new();
    for (i, mesh) in model.meshes.into_iter().enumerate() {
        // build key using path and primitive index so different meshes in the
        // same file don't collide.
        let key = format!("{}#{}", path.display(), i);

        // convert to renderer vertices
        let mut verts = Vec::with_capacity(mesh.positions.len());
        for j in 0..mesh.positions.len() {
            verts.push(ferrous_renderer::geometry::Vertex {
                position: mesh.positions[j],
                normal: mesh.normals[j],
                tangent: mesh.tangents[j],
                color: mesh.colors[j],
                uv: mesh.uvs[j],
            });
        }

        // we always use 32‑bit indices for simplicity; GLTF already gives us
        // u32 so no conversion is required.
        let index_format = wgpu::IndexFormat::Uint32;

        let gpu_mesh = ferrous_renderer::geometry::Mesh {
            vertex_buffer: ferrous_renderer::resources::buffer::create_vertex(
                &renderer.context.device,
                "gltf vertices",
                &verts,
            ),
            index_buffer: ferrous_renderer::resources::buffer::create_index(
                &renderer.context.device,
                "gltf indices",
                &mesh.indices,
            ),
            index_count: mesh.indices.len() as u32,
            vertex_count: verts.len() as u32,
            index_format,
        };

        // register mesh with renderer so world_sync can find it later
        renderer.register_mesh(&key, gpu_mesh.clone());

        // spawn an entity referencing the mesh and material
        // use the Vec3 re-export from ferrous_app for convenience
        let handle = world.spawn_mesh(format!("{}", key), key.clone(), Vec3::ZERO);
        if let Some(mat_idx) = mesh.material_idx {
            if let Some(mat_h) = mat_handles.get(mat_idx) {
                world.set_material_handle(handle, *mat_h);
            }
        }
        out_handles.push(handle);
    }

    Ok(out_handles)
}
