use ferrous_core::glam::{Mat4, Vec3, Vec4};
use ferrous_core::scene::{axis_vector, Axis, GizmoState, Plane};
use ferrous_core::{Handle, InputState, MouseButton, RenderStats, Time, Viewport, World};
use ferrous_renderer::scene::GizmoDraw;
use winit::window::Window;

/// Per-frame context passed to every [`FerrousApp`] callback.
///
/// `AppContext` bundles together all the read/write access a game or app
/// typically needs in one place, so method signatures stay simple:
///
/// ```rust,ignore
/// fn update(&mut self, ctx: &mut AppContext) {
///     ctx.world.set_position(self.player, ctx.time.delta * speed);
///     if ctx.input.just_pressed(KeyCode::Escape) {
///         ctx.request_exit();
///     }
/// }
/// ```
pub struct AppContext<'a> {
    // ── Read-only ──────────────────────────────────────────────────────────
    /// Keyboard and mouse state for this frame.
    pub input: &'a InputState,

    /// Frame timing: delta, elapsed, FPS.
    pub time: Time,

    /// Current window size in physical pixels.
    pub window_size: (u32, u32),

    /// The native window handle (for creating surfaces, grabbing cursor, etc.)
    pub window: &'a Window,

    /// Per-frame renderer statistics (vertices, triangles, draw calls).
    /// Populated at the start of every `draw_3d` call; zero before the first frame.
    pub render_stats: RenderStats,

    /// World-space position of the camera eye this frame.
    /// Populated at the start of every `draw_3d` call; `Vec3::ZERO` until then.
    pub camera_eye: Vec3,

    /// World-space position of the camera's look-at target this frame.
    /// Populated at the start of every `draw_3d` call; `Vec3::ZERO` until then.
    /// Use this together with `camera_eye` to compute the view direction.
    pub camera_target: Vec3,

    // ── Read-write ─────────────────────────────────────────────────────────
    /// The scene graph.  Modify this in `update()` and `ferrous_app` will
    /// automatically call `renderer.sync_world` at the right moment.
    pub world: &'a mut World,

    /// Area of the window dedicated to 3-D rendering.  Set this in `update()`
    /// to control where the 3-D view appears; the runner will propagate it to
    /// the renderer and UI viewport.
    pub viewport: Viewport,

    /// Gizmos queued for rendering this frame.
    ///
    /// Push one [`GizmoDraw`] per selected entity (or any overlay you want
    /// drawn as coloured lines) inside `draw_3d`.  The runner drains this
    /// list into the renderer after `draw_3d` returns.
    ///
    /// ```rust,ignore
    /// fn draw_3d(&mut self, ctx: &mut AppContext) {
    ///     if let Some(tr) = ctx.world.transform(self.selected) {
    ///         ctx.gizmos.push(GizmoDraw::new(tr.matrix(), GizmoMode::Translate));
    ///     }
    /// }
    /// ```
    pub gizmos: Vec<GizmoDraw>,

    /// Set to `true` via [`request_exit`] to stop the event loop gracefully.
    pub(crate) exit_requested: bool,

    /// Active GPU backend, set by the runner after GPU init.
    pub(crate) _gpu_backend: wgpu::Backend,
}

impl<'a> AppContext<'a> {
    /// Signal the event loop to shut down after the current frame.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    /// Active GPU backend as a readable string (e.g. `"WebGPU"`, `"WebGL2"`, `"Vulkan"`).
    /// Useful to show which backend is in use in a debug overlay.
    pub fn gpu_backend(&self) -> &str {
        match self._gpu_backend {
            wgpu::Backend::Vulkan => "Vulkan",
            wgpu::Backend::Metal => "Metal",
            wgpu::Backend::Dx12 => "Dx12",
            wgpu::Backend::Gl => "WebGL2",
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            _ => "Unknown",
        }
    }

    /// Shortcut: window width in physical pixels.
    #[inline]
    pub fn width(&self) -> u32 {
        self.window_size.0
    }

    /// Shortcut: window height in physical pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.window_size.1
    }

    /// Aspect ratio (width / height). Returns 1.0 if height is zero.
    #[inline]
    pub fn aspect(&self) -> f32 {
        let (w, h) = self.window_size;
        if h == 0 {
            1.0
        } else {
            w as f32 / h as f32
        }
    }

    // ── Gizmo helper ───────────────────────────────────────────────────────

    /// All-in-one gizmo update. Call this once per frame from `draw_3d` for
    /// the selected entity. It will:
    ///
    /// 1. Sync the gizmo transform to the entity's current world position.
    /// 2. Project the three axis arms into screen space using the current
    ///    camera matrices (only valid inside `draw_3d`).
    /// 3. On left-click: pick the nearest axis arm within a 24 px threshold.
    /// 4. While dragging: translate the entity along the constrained axis by
    ///    projecting `mouse_delta` onto the screen-space direction of the arm.
    /// 5. Queue a [`GizmoDraw`] so the renderer draws the handles this frame.
    ///
    /// # Example
    /// ```rust,ignore
    /// fn draw_3d(&mut self, ctx: &mut AppContext) {
    ///     // select on click (simple: just pick last_cube)
    ///     if ctx.input.button_just_pressed(MouseButton::Left) {
    ///         self.selected = self.last_cube;
    ///     }
    ///     if let Some(handle) = self.selected {
    ///         ctx.update_gizmo(handle, &mut self.gizmo);
    ///     }
    /// }
    /// ```
    pub fn update_gizmo(&mut self, handle: Handle, gizmo: &mut GizmoState) {
        // 1. Sync gizmo origin to entity position (strip scale/rotation).
        if let Some(tr) = self.world.transform(handle) {
            gizmo.update_world_transform(tr);
        }

        let (win_w, win_h) = (self.window_size.0 as f32, self.window_size.1 as f32);
        let (mx, my) = {
            let (px, py) = self.input.mouse_position();
            (px as f32, py as f32)
        };

        // Build view-projection matrix from current camera data.
        let eye = self.camera_eye;
        let target = self.camera_target;
        let fwd = if (target - eye).length() > 1e-6 {
            (target - eye).normalize()
        } else {
            Vec3::NEG_Z
        };
        let right_v = fwd.cross(Vec3::Y).normalize();
        let up_v = right_v.cross(fwd).normalize();
        let view = Mat4::look_at_rh(eye, target, up_v);
        let aspect = if win_h > 0.0 { win_w / win_h } else { 1.0 };
        let proj = Mat4::perspective_rh(45.0_f32.to_radians(), aspect, 0.1, 2000.0);
        let vp = proj * view;

        // Project a world point → screen pixels. Returns None if behind camera.
        let project = |world: Vec3| -> Option<(f32, f32)> {
            let clip = vp * Vec4::new(world.x, world.y, world.z, 1.0);
            if clip.w <= 0.0 {
                return None;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            Some((
                (ndc_x * 0.5 + 0.5) * win_w,
                (1.0 - (ndc_y * 0.5 + 0.5)) * win_h,
            ))
        };

        // Gizmo sizes driven by the style — no hardcoded constants.
        let arm_len = gizmo.style.arm_length;
        let plane_off = gizmo.style.plane_offset();
        let plane_size = gizmo.style.plane_size();
        // Origin = entity position only (no scale, no rotation applied).
        let origin = gizmo.world_transform.position;

        // 2. On left-click: pick nearest axis OR plane handle.
        if self.input.button_just_pressed(MouseButton::Left) {
            let mut best_axis: Option<Axis> = None;
            let mut best_axis_dist = 24.0_f32;
            for &axis in &[Axis::X, Axis::Y, Axis::Z] {
                let tip = origin + axis_vector(axis) * arm_len;
                if let (Some(os), Some(ts)) = (project(origin), project(tip)) {
                    let dx = ts.0 - os.0;
                    let dy = ts.1 - os.1;
                    let len2 = dx * dx + dy * dy;
                    let dist = if len2 < 1e-4 {
                        let ex = mx - os.0;
                        let ey = my - os.1;
                        (ex * ex + ey * ey).sqrt()
                    } else {
                        let t = ((mx - os.0) * dx + (my - os.1) * dy) / len2;
                        let t = t.clamp(0.0, 1.0);
                        let cx = os.0 + t * dx - mx;
                        let cy = os.1 + t * dy - my;
                        (cx * cx + cy * cy).sqrt()
                    };
                    if dist < best_axis_dist {
                        best_axis_dist = dist;
                        best_axis = Some(axis);
                    }
                }
            }

            // Plane picking: test if mouse is inside the projected screen quad.
            let mut best_plane: Option<Plane> = None;
            'planes: for &plane in &[Plane::XY, Plane::XZ, Plane::YZ] {
                let (a, b) = plane.axes();
                let corners_world = [
                    origin + a * plane_off + b * plane_off,
                    origin + a * (plane_off + plane_size) + b * plane_off,
                    origin + a * (plane_off + plane_size) + b * (plane_off + plane_size),
                    origin + a * plane_off + b * (plane_off + plane_size),
                ];
                // Project all 4 corners; skip if any is behind camera.
                let mut sc = [(0.0_f32, 0.0_f32); 4];
                for (i, &cw) in corners_world.iter().enumerate() {
                    match project(cw) {
                        Some(p) => sc[i] = p,
                        None => continue 'planes,
                    }
                }
                // Robust point-in-quad via signed area (shoelace).
                // Works from any camera angle (CW or CCW projected winding).
                let mut quad_area = 0.0_f32;
                for i in 0..4 {
                    let j = (i + 1) % 4;
                    quad_area += sc[i].0 * sc[j].1 - sc[j].0 * sc[i].1;
                }
                let sign = quad_area.signum();
                let mut inside = true;
                for i in 0..4 {
                    let j = (i + 1) % 4;
                    let cross =
                        (sc[j].0 - sc[i].0) * (my - sc[i].1) - (sc[j].1 - sc[i].1) * (mx - sc[i].0);
                    if cross * sign < 0.0 {
                        inside = false;
                        break;
                    }
                }
                if inside {
                    best_plane = Some(plane);
                    break;
                }
            }

            // Planes take priority over axes when overlapping.
            if best_plane.is_some() {
                gizmo.highlighted_axis = None;
                gizmo.highlighted_plane = best_plane;
                gizmo.dragging = true;
            } else if best_axis.is_some() {
                gizmo.highlighted_axis = best_axis;
                gizmo.highlighted_plane = None;
                gizmo.dragging = best_axis.is_some();
            } else {
                gizmo.highlighted_axis = None;
                gizmo.highlighted_plane = None;
                gizmo.dragging = false;
            }
        }

        if self.input.button_just_released(MouseButton::Left) {
            gizmo.dragging = false;
        }

        // 3a. Axis drag — single axis translation.
        if gizmo.dragging {
            if let Some(axis) = gizmo.highlighted_axis {
                let av = axis_vector(axis);
                let tip = origin + av * arm_len;
                if let (Some(os), Some(ts)) = (project(origin), project(tip)) {
                    let sx = ts.0 - os.0;
                    let sy = ts.1 - os.1;
                    let slen = (sx * sx + sy * sy).sqrt();
                    if slen > 1.0 {
                        let (dx, dy) = self.input.mouse_delta();
                        let screen_dot = (dx * sx + dy * sy) / slen;
                        let world_delta = screen_dot / slen * arm_len;
                        self.world.translate(handle, av * world_delta);
                    }
                }
            }

            // 3b. Plane drag — two-axis translation.
            if let Some(plane) = gizmo.highlighted_plane {
                let (a, b) = plane.axes();
                let (dx, dy) = self.input.mouse_delta();
                // Project each axis tip to find their screen directions,
                // then accumulate contribution from mouse delta along each.
                let mut total = Vec3::ZERO;
                for av in [a, b] {
                    let tip = origin + av * arm_len;
                    if let (Some(os), Some(ts)) = (project(origin), project(tip)) {
                        let sx = ts.0 - os.0;
                        let sy = ts.1 - os.1;
                        let slen = (sx * sx + sy * sy).sqrt();
                        if slen > 1.0 {
                            let screen_dot = (dx * sx + dy * sy) / slen;
                            let world_delta = screen_dot / slen * arm_len;
                            total += av * world_delta;
                        }
                    }
                }
                self.world.translate(handle, total);
            }
        }

        // 4. Queue draw — position_matrix() strips entity scale so handles
        //    are always the same size regardless of object dimensions.
        let mut draw = GizmoDraw::new(gizmo.position_matrix(), gizmo.mode);
        draw.highlighted_axis = gizmo.highlighted_axis;
        draw.highlighted_plane = gizmo.highlighted_plane;
        draw.style = gizmo.style.clone();
        self.gizmos.push(draw);
    }
}
