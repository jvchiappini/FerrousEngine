use ferrous_app::{AppContext, Color, Quat, Vec3};
use ferrous_core::scene::{ElementKind, SceneBlueprint};
use crate::commands::JsCommand;
use crate::runtime::WebRuntime;

/// Executes [`JsCommand`]s against the live [`AppContext`].
///
/// Keeping command dispatch here (rather than inlined into the runtime) gives
/// a clean separation of concerns and makes it easy to test each arm in
/// isolation.
pub struct CommandDispatcher;

fn ensure_unique_material(ctx: &mut AppContext, handle: ferrous_core::scene::Handle) {
    if let Some(desc) = ctx.world.get_material_descriptor(handle).cloned() {
        let new_handle = ctx.render.create_material(&desc);
        ctx.world.set_material_handle(handle, new_handle);
    }
}

impl CommandDispatcher {
    pub fn dispatch(runtime: &mut WebRuntime, ctx: &mut AppContext, cmd: JsCommand) {
        match cmd {
            // ── Scene management ─────────────────────────────────────────────
            JsCommand::CreateScene { scene_id } => {
                runtime.scenes.entry(scene_id).or_default();
            }
            JsCommand::SetActiveScene { scene_id } => {
                runtime.scenes.entry(scene_id).or_default();
                runtime.active_scene = scene_id;
            }
            JsCommand::ClearWorld => {
                ctx.world.clear();
                for entities in runtime.scenes.values_mut() {
                    entities.clear();
                }
            }
            JsCommand::ExportScene { request_id } => {
                let blueprint = ctx.world.to_blueprint("Exported Scene");
                match serde_json::to_string(&blueprint) {
                    Ok(json) => runtime.resolve_scene_export(request_id, json),
                    Err(e) => runtime.report_error("scene.serialization_error", &e.to_string()),
                }
            }
            JsCommand::ImportScene { json } => {
                match serde_json::from_str::<SceneBlueprint>(&json) {
                    Ok(blueprint) => {
                        ctx.world.from_blueprint(blueprint);
                        runtime.refresh_active_scene_from_world(ctx);
                        log::info!("[Ferrous] Scene imported successfully");
                    }
                    Err(e) => runtime.report_error("scene.deserialization_error", &e.to_string()),
                }
            }

            // ── Entity lifecycle ─────────────────────────────────────────────
            JsCommand::RemoveEntity { name } => {
                if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                    ctx.world.despawn(handle);
                } else {
                    runtime.report_error(
                        "entity.not_found",
                        &format!("Cannot remove entity: '{}' not found", name),
                    );
                }
                runtime.remove_entity_from_scenes(&name);
            }
            JsCommand::SetVisible { name, visible } => {
                if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                    ctx.world.set_visible(handle, visible);
                } else {
                    runtime.report_error(
                        "entity.not_found",
                        &format!("Cannot set visibility: '{}' not found", name),
                    );
                }
            }
            JsCommand::SpawnEntity { name, kind, position, color } => {
                let mut builder = ctx.world.spawn(name.clone());
                match kind.as_str() {
                    "Cube" => builder = builder.with_kind(ElementKind::Cube {
                        half_extents: Vec3::splat(0.5),
                    }),
                    "Sphere" => builder = builder.with_kind(ElementKind::Sphere {
                        radius: 0.5, latitudes: 12, longitudes: 16,
                    }),
                    "Cylinder" => builder = builder.with_kind(ElementKind::Cylinder {
                        radius_top: 0.5, radius_bottom: 0.5, height: 1.0,
                        radial_segments: 16, height_segments: 1, open_ended: false,
                    }),
                    "Cone" => builder = builder.with_kind(ElementKind::Cylinder {
                        radius_top: 0.0, radius_bottom: 0.5, height: 1.0,
                        radial_segments: 16, height_segments: 1, open_ended: false,
                    }),
                    "Torus" => builder = builder.with_kind(ElementKind::Torus {
                        radius: 0.5, tube: 0.2, radial_segments: 16, tubular_segments: 24,
                    }),
                    "Plane" => builder = builder.with_kind(ElementKind::Plane {
                        width: 1.0, height: 1.0, width_segments: 1, height_segments: 1,
                    }),
                    "Capsule" => builder = builder.with_kind(ElementKind::Capsule {
                        radius: 0.5, height: 1.0, radial_segments: 16, cap_segments: 8,
                    }),
                    "Circle" => builder = builder.with_kind(ElementKind::Circle {
                        radius: 0.5, segments: 32,
                    }),
                    "Ring" => builder = builder.with_kind(ElementKind::Ring {
                        inner_radius: 0.25, outer_radius: 0.5, segments: 32, rings: 1,
                    }),
                    mesh_key => builder = builder.with_kind(ElementKind::Mesh {
                        asset_key: mesh_key.to_string(),
                    }),
                }
                let handle = builder
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }

            // ── Primitives ───────────────────────────────────────────────────
            JsCommand::CreateSprite2d { name, position, size, z_index, color, texture_id } => {
                ctx.ecs.spawn((
                    ferrous_2d::components::Transform2d {
                        position: glam::Vec2::new(position[0], position[1]),
                        scale: glam::Vec2::new(1.0, 1.0),
                        rotation: 0.0,
                        z_index,
                    },
                    ferrous_2d::components::Sprite {
                        color: glam::Vec4::new(color[0], color[1], color[2], color[3]),
                        custom_size: Some(glam::Vec2::new(size[0], size[1])),
                        texture_id,
                        ..Default::default()
                    }
                ));
                // Optional: Register to world to track it by name if required.
                // However, ECS stands alone.
            }
            JsCommand::SetCamera2d { zoom, clear_color } => {
                ctx.ecs.spawn((
                    ferrous_2d::components::Camera2d {
                        zoom,
                        clear_color: clear_color.map(|c| glam::Vec4::new(c[0], c[1], c[2], c[3])),
                    },
                ));
            }
            JsCommand::CreateBox { name, position, size, color } => {
                let handle = ctx.world.spawn_box(
                    name.clone(),
                    Vec3::from_array(position),
                    Vec3::from_array(size),
                );
                ctx.world.set_color(handle, Color::rgb(color[0], color[1], color[2]));
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateSphere { name, position, radius, segments, color } => {
                let handle = ctx.world.spawn_sphere(
                    name.clone(),
                    Vec3::from_array(position),
                    radius,
                    segments,
                );
                ctx.world.set_color(handle, Color::rgb(color[0], color[1], color[2]));
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateCylinder {
                name, position, radius_top, radius_bottom, height,
                radial_segments, open_ended, color,
            } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Cylinder {
                        radius_top, radius_bottom, height,
                        radial_segments, height_segments: 1, open_ended,
                    })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateCone { name, position, radius, height, radial_segments, color } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Cylinder {
                        radius_top: 0.0, radius_bottom: radius, height,
                        radial_segments, height_segments: 1, open_ended: false,
                    })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateTorus {
                name, position, radius, tube, radial_segments, tubular_segments, color,
            } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Torus { radius, tube, radial_segments, tubular_segments })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreatePlane {
                name, position, width, height, width_segments, height_segments, color,
            } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Plane { width, height, width_segments, height_segments })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateCapsule {
                name, position, radius, height, radial_segments, cap_segments, color,
            } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Capsule { radius, height, radial_segments, cap_segments })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateCircle { name, position, radius, segments, color } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Circle { radius, segments })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateRing {
                name, position, inner_radius, outer_radius, segments, rings, color,
            } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Ring { inner_radius, outer_radius, segments, rings })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                ensure_unique_material(ctx, handle);
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::CreateText3D {
                name, text, font_data, position, depth, bevel_enabled, bevel_thickness, bevel_size, quality, color,
            } => {
                let handle = ctx.world.spawn(name.clone())
                    .with_kind(ElementKind::Text3D { text, font_data, depth, bevel_enabled, bevel_thickness, bevel_size, quality: quality as u8 })
                    .with_position(Vec3::from_array(position))
                    .with_color(Color::rgb(color[0], color[1], color[2]))
                    .build();
                
                // Force double-sided material for text to ensure all extrusion sides are visible
                let mut desc = ctx.world.get_material_descriptor(handle).cloned().unwrap_or_default();
                desc.double_sided = true;
                let new_handle = ctx.render.create_material(&desc);
                ctx.world.set_material_handle(handle, new_handle);
                ctx.world.set_material_descriptor(handle, desc);

                runtime.add_entity_to_active_scene(name);
            }

            // ── Transform ────────────────────────────────────────────────────
            JsCommand::SetPosition { name, position } => {
                if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                    ctx.world.set_position(handle, Vec3::from_array(position));
                }
            }
            JsCommand::SetRotation { name, rotation } => {
                if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                    let rot = Quat::from_rotation_y(rotation[1])
                        * Quat::from_rotation_x(rotation[0])
                        * Quat::from_rotation_z(rotation[2]);
                    ctx.world.set_rotation(handle, rot);
                }
            }
            JsCommand::SetScale { name, scale } => {
                if let Some(handle) = ctx.world.find_entity_by_name(&name) {
                    ctx.world.set_scale(handle, Vec3::from_array(scale));
                }
            }

            // ── Camera ───────────────────────────────────────────────────────
            JsCommand::SetCamera { eye, target } => {
                runtime.camera.set_camera(eye, target);
            }
            JsCommand::SetCameraControlMode { mode } => {
                if let Some(parsed_mode) = crate::config::CameraControlMode::parse(&mode) {
                    runtime.camera.set_mode(parsed_mode);
                } else {
                    runtime.report_error(
                        "camera.invalid_mode",
                        &format!("Invalid camera mode: '{}'. Valid: fly | orbit | none", mode),
                    );
                }
            }
            JsCommand::SetCameraParams { speed, sensitivity } => {
                runtime.camera.set_move_speed(speed);
                runtime.camera.set_look_sensitivity(sensitivity);
            }
            JsCommand::SetCameraFov { fov_degrees } => {
                ctx.render.renderer_mut().camera_mut().fovy = fov_degrees.to_radians();
            }

            // ── Lighting ─────────────────────────────────────────────────────
            JsCommand::AddPointLight { name, position, color, intensity, range } => {
                ctx.world.spawn_point_light(
                    name.clone(),
                    Vec3::from_array(position),
                    color,
                    intensity,
                    range.max(0.01),
                );
                runtime.add_entity_to_active_scene(name);
            }
            JsCommand::SetDirectionalLight { direction, color, intensity } => {
                ctx.render.set_directional_light(direction, color, intensity);
            }
            JsCommand::SetAmbientLight { color, intensity } => {
                ctx.render.set_ambient_light(color, intensity);
                log::info!("[dispatcher] setAmbientLight applied: color={:?}, intensity={}", color, intensity);
            }

            // ── Materials ────────────────────────────────────────────────────
            JsCommand::UpdateMaterial {
                entity_name, r, g, b, metallic, roughness,
                clearcoat, clearcoat_roughness, opacity, albedo_tex,
            } => {
                if let Some(handle) = ctx.world.find_entity_by_name(&entity_name) {
                    let mut desc = ctx.world
                        .get_material_descriptor(handle)
                        .cloned()
                        .unwrap_or_default();
                    desc.base_color = [r, g, b, 1.0];
                    desc.metallic = metallic;
                    desc.roughness = roughness;
                    desc.clearcoat = clearcoat;
                    desc.clearcoat_roughness = clearcoat_roughness;
                    desc.opacity = opacity;
                    if let Some(tex_id) = albedo_tex {
                        desc.albedo_tex = Some(tex_id);
                    }
                    ctx.world.set_material_descriptor(handle, desc.clone());
                    
                    // Force the renderer to allocate a NEW handle so it doesn't overwrite others
                    let new_mat_handle = ctx.render.create_material(&desc);
                    ctx.world.set_material_handle(handle, new_mat_handle);

                    // Update the ECS component to prevent sync_world from overwriting this!
                    if let Some(entity) = ctx.world.ecs_mapping.get(&handle.0) {
                        if let Some(mut m) = ctx.world.ecs.get_mut::<ferrous_core::scene::Material>(*entity) {
                            m.base_color = ferrous_app::Color::rgba(r, g, b, 1.0);
                            m.metallic = metallic;
                            m.roughness = roughness;
                            m.clearcoat = clearcoat;
                            m.clearcoat_roughness = clearcoat_roughness;
                            m.opacity = opacity;
                        }
                    }
                } else {
                    runtime.report_error(
                        "entity.not_found",
                        &format!("Cannot update material: '{}' not found", entity_name),
                    );
                }
            }

            // ── Environment ──────────────────────────────────────────────────
            JsCommand::SetEnvironment { fog_color, fog_density } => {
                ctx.render.set_fog(fog_color, fog_density);
            }
            JsCommand::SetExposure { exposure } => {
                ctx.render.set_exposure(exposure);
            }
            JsCommand::SetBackground { r, g, b } => {
                ctx.render.set_clear_color(Color::rgb(r, g, b));
            }

            // ── Assets ───────────────────────────────────────────────────────
            JsCommand::LoadTexture { url, request_id } => {
                let handle = ctx.asset_server.load::<ferrous_assets::ImageData>(url);
                runtime.pending_textures.insert(request_id, handle);
            }
            JsCommand::LoadModel { url, request_id } => {
                let handle = ctx.asset_server.load::<ferrous_assets::GltfModel>(url);
                runtime.pending_models.insert(request_id, handle);
            }

            // ── Debug / Plugins ──────────────────────────────────────────────
            JsCommand::SetDebugMode { enabled } => {
                runtime.debug_mode = enabled;
            }
            JsCommand::EnablePlugin { name } => {
                runtime.enabled_plugins.lock().unwrap().insert(name);
            }
            JsCommand::DisablePlugin { name } => {
                runtime.enabled_plugins.lock().unwrap().remove(&name);
            }
            JsCommand::SetSsaoParams { radius, bias, intensity, power } => {
                ctx.render.set_ssao_params(radius, bias, intensity, power);
            }
            JsCommand::LegacyCreateTerrain => {
                if runtime.is_plugin_enabled("terrain") {
                    runtime.apply_legacy_terrain(ctx);
                } else {
                    runtime.report_error(
                        "plugin.disabled",
                        "Cannot create terrain: 'terrain' plugin is disabled",
                    );
                }
            }
            JsCommand::LegacyToggleSky => {
                if runtime.is_plugin_enabled("sky") {
                    ctx.render.renderer_mut().set_sky_procedural();
                } else {
                    runtime.report_error(
                        "plugin.disabled",
                        "Cannot toggle sky: 'sky' plugin is disabled",
                    );
                }
            }
        }
    }
}
