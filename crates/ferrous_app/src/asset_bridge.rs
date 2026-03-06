use crate::Vec3;
use anyhow::Result;
use ferrous_assets::{AssetHandle, AssetState, AssetServer, GltfModel};
use std::path::Path;

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

    // diagnostic: dump model summary
    eprintln!(
        "loaded gltf '{}' -> {} meshes, {} materials, {} images",
        path.display(),
        model.meshes.len(),
        model.materials.len(),
        model.images.len()
    );

    // Determine the correct color space for each image index by scanning
    // all materials.  GLTF spec mandates:
    //   • albedo (base_color) and emissive  → sRGB
    //   • normal, metallic-roughness, AO    → linear
    // An image might appear in multiple slots of different materials, but
    // in practice GLTF files never share the same image between a color
    // slot and a data slot, so one pass over all materials is enough.
    let n_images = model.images.len();
    // true = must be registered as linear (non-color data)
    let mut linear_flag = vec![false; n_images];
    for raw in &model.materials {
        if let Some(i) = raw.normal_tex {
            if i < n_images {
                linear_flag[i] = true;
            }
        }
        if let Some(i) = raw.metallic_roughness_tex {
            if i < n_images {
                linear_flag[i] = true;
            }
        }
        if let Some(i) = raw.ao_tex {
            if i < n_images {
                linear_flag[i] = true;
            }
        }
    }

    // Register images with the renderer, choosing the correct format for
    // each image based on whether it carries color or data.
    let mut tex_handles = Vec::with_capacity(n_images);
    for (img_idx, (w, h, pixels)) in model.images.iter().enumerate() {
        let th = if linear_flag[img_idx] {
            eprintln!(" image {} -> linear (normal/MR/AO data)", img_idx);
            renderer.register_texture_linear(*w, *h, pixels)
        } else {
            eprintln!(" image {} -> sRGB (color)", img_idx);
            renderer.register_texture(*w, *h, pixels)
        };
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
        desc.alpha_mode = match raw.alpha_mode {
            ferrous_assets::gltf_loader::AlphaMode::Opaque =>
                ferrous_core::scene::AlphaMode::Opaque,
            ferrous_assets::gltf_loader::AlphaMode::Mask { cutoff } =>
                ferrous_core::scene::AlphaMode::Mask { cutoff },
            ferrous_assets::gltf_loader::AlphaMode::Blend =>
                ferrous_core::scene::AlphaMode::Blend,
        };
        desc.double_sided = raw.double_sided;

        if let Some(idx) = raw.base_color_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.albedo_tex = Some(tex.0);
                eprintln!(" material base_color_tex -> image {} handle {}", idx, tex.0);
            }
        }
        if let Some(idx) = raw.normal_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.normal_tex = Some(tex.0);
                eprintln!(" material normal_tex -> image {} handle {}", idx, tex.0);
            }
        }
        if let Some(idx) = raw.metallic_roughness_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.metallic_roughness_tex = Some(tex.0);
                eprintln!(
                    " material metallic_roughness_tex -> image {} handle {}",
                    idx, tex.0
                );
            }
        }
        if let Some(idx) = raw.emissive_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.emissive_tex = Some(tex.0);
                eprintln!(" material emissive_tex -> image {} handle {}", idx, tex.0);
            }
        }
        if let Some(idx) = raw.ao_tex {
            if let Some(tex) = tex_handles.get(idx) {
                desc.ao_tex = Some(tex.0);
                eprintln!(" material ao_tex -> image {} handle {}", idx, tex.0);
            }
        }

        let mh = renderer.create_material(&desc);
        eprintln!(" created material desc: {:?} -> handle {:?}", desc, mh);
        // keep (handle, descriptor) together so we can store the full
        // descriptor on the world entity — sync_world calls update_params
        // every frame with the descriptor stored in the world, which would
        // overwrite the material uniform (clearing the texture flags) if we
        // only stored the default descriptor there.
        mat_handles.push((mh, desc));
    }

    let mut out_handles = Vec::new();
    for (i, mesh) in model.meshes.into_iter().enumerate() {
        eprintln!(
            " mesh {}: {} vertices {} indices mat_idx={:?}",
            i,
            mesh.positions.len(),
            mesh.indices.len(),
            mesh.material_idx
        );
        // build key using path and primitive index so different meshes in the
        // same file don't collide.
        let key = format!("{}#{}", path.display(), i);

        // convert to renderer vertices
        let n = mesh.positions.len();
        let mut verts = Vec::with_capacity(n);
        for j in 0..n {
            verts.push(ferrous_renderer::geometry::Vertex {
                position: mesh.positions[j],
                normal: *mesh.normals.get(j).unwrap_or(&[0.0, 1.0, 0.0]),
                tangent: *mesh.tangents.get(j).unwrap_or(&[1.0, 0.0, 0.0, 1.0]),
                color: *mesh.colors.get(j).unwrap_or(&[1.0, 1.0, 1.0]),
                uv: *mesh.uvs.get(j).unwrap_or(&[0.0, 0.0]),
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

        // spawn an entity referencing the mesh and material.
        // Crucially we also store the full descriptor (including texture
        // handles) on the world entity so that sync_world's update_params
        // call keeps the GPU uniform buffer consistent with the bind group.
        let handle = world.spawn_mesh(format!("{}", key), key.clone(), Vec3::ZERO);
        if let Some(mat_idx) = mesh.material_idx {
            if let Some((mat_h, mat_desc)) = mat_handles.get(mat_idx) {
                world.set_material_handle(handle, *mat_h);
                world.set_material_descriptor(handle, mat_desc.clone());
            }
        }
        out_handles.push(handle);
    }

    Ok(out_handles)
}

// ── Async API ────────────────────────────────────────────────────────────────

/// State of an async GLTF spawn operation initiated with [`spawn_gltf_async`].
///
/// Poll this each frame by calling [`GltfSpawnTask::poll`]; once it returns
/// `Some(handles)` the entities have been registered with the world and
/// renderer and the task can be dropped.
pub struct GltfSpawnTask {
    handle: AssetHandle<GltfModel>,
}

impl GltfSpawnTask {
    /// Returns `true` while the file is still being loaded in the background.
    pub fn is_loading(&self, server: &mut AssetServer) -> bool {
        matches!(server.get(self.handle), AssetState::Loading)
    }

    /// Poll for completion.  When the asset finishes loading this performs the
    /// GPU registration (meshes, textures, materials) and entity spawning, then
    /// returns `Some(handles)`.
    ///
    /// Returns `None` while still loading.  Returns `None` and logs an error if
    /// the asset failed to load.
    pub fn poll(
        &self,
        server: &mut AssetServer,
        world: &mut ferrous_core::scene::World,
        renderer: &mut ferrous_renderer::Renderer,
        path: &str,
    ) -> Option<Vec<ferrous_core::scene::Handle>> {
        match server.get(self.handle) {
            AssetState::Loading => None,
            AssetState::NotFound => {
                eprintln!("[GltfSpawnTask] handle not found for '{path}'");
                None
            }
            AssetState::Failed(msg) => {
                eprintln!("[GltfSpawnTask] failed to load '{path}': {msg}");
                // Return empty vec so callers know loading finished (with error).
                Some(Vec::new())
            }
            AssetState::Ready(gltf_model) => {
                // Delegate to the synchronous helper with the already-loaded model.
                match spawn_gltf_from_model(world, renderer, path, &gltf_model.0) {
                    Ok(handles) => Some(handles),
                    Err(e) => {
                        eprintln!("[GltfSpawnTask] entity spawn failed for '{path}': {e}");
                        Some(Vec::new())
                    }
                }
            }
        }
    }
}

/// Begin loading a GLTF/GLB file asynchronously via the [`AssetServer`].
///
/// Returns a [`GltfSpawnTask`] that you should store and poll each frame.
/// When the file finishes loading, call [`GltfSpawnTask::poll`] to perform
/// the GPU registration and entity spawning.
///
/// ```rust,ignore
/// // In setup():
/// self.player_task = Some(spawn_gltf_async(&mut ctx.asset_server, "assets/player.glb"));
///
/// // In update():
/// if let Some(task) = &self.player_task {
///     if let Some(handles) = task.poll(
///         &mut ctx.asset_server, &mut ctx.world, &mut ctx.renderer, "assets/player.glb"
///     ) {
///         self.player_entities = handles;
///         self.player_task = None;
///     }
/// }
/// ```
pub fn spawn_gltf_async(server: &mut AssetServer, path: &str) -> GltfSpawnTask {
    let handle = server.load::<GltfModel>(path);
    GltfSpawnTask { handle }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Perform GPU registration and entity spawning from an already-loaded
/// [`ferrous_assets::AssetModel`].  Extracted so both the sync and async paths
/// can share this logic.
fn spawn_gltf_from_model(
    world: &mut ferrous_core::scene::World,
    renderer: &mut ferrous_renderer::Renderer,
    path: &str,
    model: &ferrous_assets::AssetModel,
) -> Result<Vec<ferrous_core::scene::Handle>> {
    let path_obj = Path::new(path);

    eprintln!(
        "spawning gltf '{}' -> {} meshes, {} materials, {} images",
        path,
        model.meshes.len(),
        model.materials.len(),
        model.images.len()
    );

    let n_images = model.images.len();
    let mut linear_flag = vec![false; n_images];
    for raw in &model.materials {
        if let Some(i) = raw.normal_tex {
            if i < n_images { linear_flag[i] = true; }
        }
        if let Some(i) = raw.metallic_roughness_tex {
            if i < n_images { linear_flag[i] = true; }
        }
        if let Some(i) = raw.ao_tex {
            if i < n_images { linear_flag[i] = true; }
        }
    }

    let mut tex_handles = Vec::with_capacity(n_images);
    for (img_idx, (w, h, pixels)) in model.images.iter().enumerate() {
        let th = if linear_flag[img_idx] {
            renderer.register_texture_linear(*w, *h, pixels)
        } else {
            renderer.register_texture(*w, *h, pixels)
        };
        tex_handles.push(th);
    }

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
        desc.alpha_mode = match raw.alpha_mode {
            ferrous_assets::gltf_loader::AlphaMode::Opaque =>
                ferrous_core::scene::AlphaMode::Opaque,
            ferrous_assets::gltf_loader::AlphaMode::Mask { cutoff } =>
                ferrous_core::scene::AlphaMode::Mask { cutoff },
            ferrous_assets::gltf_loader::AlphaMode::Blend =>
                ferrous_core::scene::AlphaMode::Blend,
        };
        desc.double_sided = raw.double_sided;
        if let Some(idx) = raw.base_color_tex {
            if let Some(tex) = tex_handles.get(idx) { desc.albedo_tex = Some(tex.0); }
        }
        if let Some(idx) = raw.normal_tex {
            if let Some(tex) = tex_handles.get(idx) { desc.normal_tex = Some(tex.0); }
        }
        if let Some(idx) = raw.metallic_roughness_tex {
            if let Some(tex) = tex_handles.get(idx) { desc.metallic_roughness_tex = Some(tex.0); }
        }
        if let Some(idx) = raw.emissive_tex {
            if let Some(tex) = tex_handles.get(idx) { desc.emissive_tex = Some(tex.0); }
        }
        if let Some(idx) = raw.ao_tex {
            if let Some(tex) = tex_handles.get(idx) { desc.ao_tex = Some(tex.0); }
        }
        let mh = renderer.create_material(&desc);
        mat_handles.push((mh, desc));
    }

    let mut out_handles = Vec::new();
    for (i, mesh) in model.meshes.iter().enumerate() {
        let key = format!("{}#{}", path_obj.display(), i);
        let n = mesh.positions.len();
        let mut verts = Vec::with_capacity(n);
        for j in 0..n {
            verts.push(ferrous_renderer::geometry::Vertex {
                position: mesh.positions[j],
                normal: *mesh.normals.get(j).unwrap_or(&[0.0, 1.0, 0.0]),
                tangent: *mesh.tangents.get(j).unwrap_or(&[1.0, 0.0, 0.0, 1.0]),
                color: *mesh.colors.get(j).unwrap_or(&[1.0, 1.0, 1.0]),
                uv: *mesh.uvs.get(j).unwrap_or(&[0.0, 0.0]),
            });
        }
        let gpu_mesh = ferrous_renderer::geometry::Mesh {
            vertex_buffer: ferrous_renderer::resources::buffer::create_vertex(
                &renderer.context.device, "gltf vertices", &verts,
            ),
            index_buffer: ferrous_renderer::resources::buffer::create_index(
                &renderer.context.device, "gltf indices", &mesh.indices,
            ),
            index_count: mesh.indices.len() as u32,
            vertex_count: verts.len() as u32,
            index_format: wgpu::IndexFormat::Uint32,
        };
        renderer.register_mesh(&key, gpu_mesh.clone());

        let handle = world.spawn_mesh(key.clone(), key.clone(), Vec3::ZERO);
        if let Some(mat_idx) = mesh.material_idx {
            if let Some((mat_h, mat_desc)) = mat_handles.get(mat_idx) {
                world.set_material_handle(handle, *mat_h);
                world.set_material_descriptor(handle, mat_desc.clone());
            }
        }
        out_handles.push(handle);
    }

    Ok(out_handles)
}
