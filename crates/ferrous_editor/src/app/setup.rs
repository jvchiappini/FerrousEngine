//! `EditorApp::run_setup` — populates the PBR test scene on app start.

use ferrous_app::{AppContext, Color, DirectionalLight, OrbitCamera, Vec3};
use ferrous_core::scene::{AlphaMode, ElementKind, MaterialDescriptor};
use glam::Quat;

use super::types::EditorApp;

impl EditorApp {
    pub(super) fn run_setup(&mut self, ctx: &mut AppContext) {
        // ── PBR test scene ──────────────────────────────────────────────────
        struct Preset {
            name: &'static str,
            base_color: [f32; 4],
            metallic: f32,
            roughness: f32,
            emissive_strength: f32,
        }

        let presets: &[Preset] = &[
            Preset { name: "Dielectric Matte",      base_color: [0.8, 0.8, 0.8, 1.0], metallic: 0.0, roughness: 1.0, emissive_strength: 0.0 },
            Preset { name: "Dielectric Semi-rough",  base_color: [0.8, 0.8, 0.8, 1.0], metallic: 0.0, roughness: 0.5, emissive_strength: 0.0 },
            Preset { name: "Dielectric Smooth",      base_color: [0.8, 0.8, 0.8, 1.0], metallic: 0.0, roughness: 0.1, emissive_strength: 0.0 },
            Preset { name: "Metal Rough",            base_color: [1.0, 0.76, 0.33, 1.0], metallic: 1.0, roughness: 0.8, emissive_strength: 0.0 },
            Preset { name: "Metal Semi-rough",       base_color: [1.0, 0.76, 0.33, 1.0], metallic: 1.0, roughness: 0.4, emissive_strength: 0.0 },
            Preset { name: "Metal Smooth",           base_color: [1.0, 0.76, 0.33, 1.0], metallic: 1.0, roughness: 0.1, emissive_strength: 0.0 },
            Preset { name: "Metal Mirror",           base_color: [1.0, 0.76, 0.33, 1.0], metallic: 1.0, roughness: 0.0, emissive_strength: 0.0 },
        ];

        // Convert sRGB [0..1] RGBA to linear RGBA.
        // Prefer Color::srgb() for new code; this is kept for array-typed preset data.
        fn to_linear(c: [f32; 4]) -> [f32; 4] {
            [c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2), c[3]]
        }

        let spacing = 1.6_f32;
        let total = presets.len() as f32;
        let x_start = -(total - 1.0) * spacing * 0.5;

        for (i, preset) in presets.iter().enumerate() {
            let x = x_start + i as f32 * spacing;
            let mut desc = MaterialDescriptor::default();
            desc.base_color = to_linear(preset.base_color);
            desc.metallic = preset.metallic;
            desc.roughness = preset.roughness;
            desc.emissive_strength = preset.emissive_strength;
            if preset.emissive_strength > 0.0 {
                desc.emissive = [1.0, 0.4, 0.1];
            }
            let mat = ctx.renderer.create_material(&desc);
            let h = ctx
                .world
                .spawn(preset.name)
                .with_position(Vec3::new(x, 0.0, 0.0))
                .with_kind(ElementKind::Cube { half_extents: Vec3::splat(0.5) })
                .with_scale(Vec3::splat(0.5))
                .with_material_handle(mat)
                .build();
            ctx.world.set_material_descriptor(h, desc);
            self.last_cube = Some(h);
        }

        // ── Color pair ──────────────────────────────────────────────────────
        for (name, col, x_off) in &[
            ("Red Plastic",  [0.9_f32, 0.1, 0.1, 1.0], -1.0_f32),
            ("Blue Plastic", [0.1_f32, 0.3, 0.9, 1.0],  1.0_f32),
        ] {
            let mut desc = MaterialDescriptor::default();
            desc.base_color = to_linear(*col);
            desc.metallic = 0.0;
            desc.roughness = 0.3;
            let mat = ctx.renderer.create_material(&desc);
            let h = ctx
                .world
                .spawn(*name)
                .with_position(Vec3::new(*x_off, -1.6, 0.0))
                .with_kind(ElementKind::Cube { half_extents: Vec3::splat(0.5) })
                .with_scale(Vec3::splat(0.5))
                .with_material_handle(mat)
                .build();
            ctx.world.set_material_descriptor(h, desc);
        }

        // ── Mirror sphere ───────────────────────────────────────────────────
        {
            let mut desc = MaterialDescriptor::default();
            desc.metallic = 1.0;
            let mat = ctx.renderer.create_material(&desc);
            let hs = ctx.world.spawn_sphere("Sphere", Vec3::new(2.0, 0.0, 0.0), 1.0, 32);
            ctx.world.set_material_handle(hs, mat);
            ctx.world.set_material_descriptor(hs, desc);
        }

        // ── Directional light (ECS — Phase 4.5 API) ─────────────────────────
        let ldir = Vec3::new(-0.6, -0.8, -0.4).normalize();
        ctx.world.ecs.spawn((DirectionalLight {
            direction: ldir,
            color: Color::WARM_WHITE,
            intensity: 3.5,
        },));

        // ── Orbit camera (ECS — Phase 4.5 API) ──────────────────────────────
        ctx.world.ecs.spawn((OrbitCamera {
            yaw: -0.52,
            pitch: 0.35,
            distance: ctx.renderer.camera().controller.orbit_distance,
            target: ctx.renderer.camera().target,
        },));
        {
            let yaw = -0.52_f32;
            let pitch = 0.35_f32;
            let dist = ctx.renderer.camera().controller.orbit_distance;
            let cy = pitch.cos();
            let sy = pitch.sin();
            let offset = Vec3::new(yaw.sin() * cy, sy, yaw.cos() * cy) * dist;
            let target = ctx.renderer.camera().target;
            ctx.renderer.camera_mut().eye = target + offset;
        }

        self.gpu_backend = ctx.gpu_backend().to_string();

        // ── Optional test model ──────────────────────────────────────────────
        let test_model =
            r"C:\Users\jvchi\CARPETAS\FerrousEngine\assets\models\DamagedHelmet.glb";
        if std::path::Path::new(test_model).exists() {
            if let Ok(handles) =
                ferrous_app::spawn_gltf(&mut ctx.world, &mut ctx.renderer, test_model)
            {
                log::info!("spawned {} meshes from {}", handles.len(), test_model);
                for h in &handles {
                    ctx.world.set_position(*h, Vec3::new(5.0, 2.5, 0.0));
                    ctx.world.set_rotation(*h, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
                }
            } else {
                log::warn!("failed to load glTF from {}", test_model);
            }
        } else {
            for p in &["model.gltf", "model.glb"] {
                if std::path::Path::new(p).exists() {
                    if let Ok(handles) =
                        ferrous_app::spawn_gltf(&mut ctx.world, &mut ctx.renderer, p)
                    {
                        log::info!("spawned {} meshes from {}", handles.len(), p);
                    } else {
                        log::warn!("failed to load glTF from {}", p);
                    }
                    break;
                }
            }
        }

        // ── HDR point lights ─────────────────────────────────────────────────
        let helmet_center = Vec3::new(0.0, 2.5, 0.0);
        let offset = 1.5_f32;
        ctx.world.spawn_point_light("PointLight Red",    helmet_center + Vec3::new( offset, 0.0,    0.0), [1.0, 0.05, 0.05], 80.0, 8.0);
        ctx.world.spawn_point_light("PointLight Blue",   helmet_center + Vec3::new(-offset, 0.0,    0.0), [0.05, 0.1, 1.0],  80.0, 8.0);
        ctx.world.spawn_point_light("PointLight Green",  helmet_center + Vec3::new( 0.0,    0.0,  offset), [0.05, 1.0, 0.05], 80.0, 8.0);
        ctx.world.spawn_point_light("PointLight Yellow", helmet_center + Vec3::new( 0.0,    0.0, -offset), [1.0, 0.9, 0.1],   60.0, 7.0);

        // ── Transparency test scene ──────────────────────────────────────────
        // Background: 3 opaque cubes
        for (name, color, x_pos) in &[
            ("BG Cube Red",   [1.0_f32, 0.0, 0.0, 1.0], -2.0_f32),
            ("BG Cube Green", [0.0_f32, 1.0, 0.0, 1.0],  0.0_f32),
            ("BG Cube Blue",  [0.0_f32, 0.0, 1.0, 1.0],  2.0_f32),
        ] {
            let mut desc = MaterialDescriptor::default();
            desc.base_color = *color;
            desc.metallic = 0.0;
            desc.roughness = 0.8;
            desc.alpha_mode = AlphaMode::Opaque;
            let mat = ctx.renderer.create_material(&desc);
            let h = ctx
                .world
                .spawn(*name)
                .with_position(Vec3::new(*x_pos, 0.0, -4.0))
                .with_kind(ElementKind::Cube { half_extents: Vec3::splat(0.5) })
                .with_scale(Vec3::splat(0.5))
                .with_material_handle(mat)
                .build();
            ctx.world.set_material_descriptor(h, desc);
        }

        // Mid layer: cyan glass sphere
        {
            let mut desc = MaterialDescriptor::default();
            desc.base_color = [0.2, 0.8, 1.0, 0.5];
            desc.metallic = 0.0;
            desc.roughness = 0.1;
            desc.alpha_mode = AlphaMode::Blend;
            desc.double_sided = true;
            let mat = ctx.renderer.create_material(&desc);
            let hs = ctx.world.spawn_sphere(
                "Glass Sphere (Cyan)", Vec3::new(0.0, 0.0, -2.0), 0.6, 32,
            );
            ctx.world.set_material_handle(hs, mat);
            ctx.world.set_material_descriptor(hs, desc);
        }

        // Front layer: red glass cube
        {
            let mut desc = MaterialDescriptor::default();
            desc.base_color = [1.0, 0.2, 0.2, 0.5];
            desc.metallic = 0.0;
            desc.roughness = 0.1;
            desc.alpha_mode = AlphaMode::Blend;
            desc.double_sided = true;
            let mat = ctx.renderer.create_material(&desc);
            let h = ctx
                .world
                .spawn("Glass Cube (Red)")
                .with_position(Vec3::new(0.0, 0.0, 0.0))
                .with_kind(ElementKind::Cube { half_extents: Vec3::splat(0.5) })
                .with_scale(Vec3::splat(0.5))
                .with_material_handle(mat)
                .build();
            ctx.world.set_material_descriptor(h, desc);
        }
    }
}
