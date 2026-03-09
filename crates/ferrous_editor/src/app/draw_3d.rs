//! `EditorApp::run_draw_3d` — gizmo interaction, cube spawning, resize handler.

use ferrous_app::{AppContext, Color, Vec3, Viewport};
use ferrous_core::scene::{GizmoMode, MaterialDescriptor};
use rand::Rng;

use super::types::EditorApp;

impl EditorApp {
    pub(super) fn run_draw_3d(&mut self, ctx: &mut AppContext) {
        self.cached_render_stats = ctx.render_stats;

        // Resize last cube using slider values
        if let Some(handle) = self.last_cube {
            if ctx.world.contains(handle) {
                ctx.world.set_cube_size(handle, self.cube_size);
            }
        }

        let mut rng = rand::thread_rng();

        if self.add_cube {
            let base = ctx.camera_eye;
            let pos = Vec3::new(
                base.x + (rng.gen::<f32>() - 0.5) * 2.0,
                base.y + (rng.gen::<f32>() - 0.5) * 2.0,
                base.z - 5.0 + (rng.gen::<f32>() - 0.5),
            );
            let handle = ctx.world.spawn_cube("Cube", pos);
            let color = Color::from_rgb8(
                rng.gen_range(100..=255),
                rng.gen_range(100..=255),
                rng.gen_range(100..=255),
            );
            let mut desc = MaterialDescriptor::default();
            desc.base_color = color.to_array();
            let mat = ctx.render.create_material(&desc);
            ctx.world.set_material_handle(handle, mat);
            ctx.world.set_material_descriptor(handle, desc.clone());
            let qh = ctx.world.spawn_quad(
                "Quad", pos + Vec3::new(0.0, 0.0, 1.0), 0.5, 0.5, true,
            );
            ctx.world.set_material_handle(qh, mat);
            self.last_quad = Some(qh);
            self.last_cube = Some(handle);
            self.add_cube = false;
        }

        // Gizmo interaction
        if let Some(sel) = self.selected {
            let pivot_active =
                self.show_pivot_gizmo && self.gizmo.mode == GizmoMode::Rotate;

            if pivot_active {
                let mut pg = self.pivot_gizmo.clone();
                ctx.update_pivot_gizmo(sel, &mut pg, &mut self.gizmo);
                self.pivot_gizmo = pg;
            }

            ctx.update_gizmo(sel, &mut self.gizmo);
        }
    }

    pub(super) fn run_on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: new_size.0,
            height: new_size.1,
        };
    }
}
