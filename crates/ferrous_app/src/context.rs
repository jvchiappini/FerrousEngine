use ferrous_assets::AssetServer;
use ferrous_core::glam::{Mat4, Vec3, Vec4};
use ferrous_core::scene::{axis_vector, Axis, GizmoMode, GizmoState, Plane};
use ferrous_core::{Handle, InputState, MouseButton, RenderStats, Time, Viewport, World};
use ferrous_renderer::scene::GizmoDraw;
use winit::window::{ResizeDirection, Window};

use crate::render_context::RenderContext;

/// Direction from which the user is dragging a window edge or corner to resize it.
///
/// Re-exported from winit so callers don't need a direct winit dependency.
pub use winit::window::ResizeDirection as WindowResizeDirection;

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

    /// User-facing renderer facade — the primary API for controlling rendering
    /// without touching GPU internals.
    ///
    /// ```rust,ignore
    /// // Switch to cel shading
    /// ctx.render.set_style(RenderStyle::CelShaded { toon_levels: 4, outline_width: 1.5 });
    /// // Disable SSAO
    /// ctx.render.set_ssao(false);
    /// // Create a material
    /// let mat = ctx.render.create_material(&desc);
    /// ```
    ///
    /// For advanced engine-internal use, the raw renderer is accessible via
    /// `ctx.render.inner` (crate-internal only).
    pub render: RenderContext<'a>,

    /// Asset server — call `load()` to begin loading an asset in the
    /// background and `get()` to poll its state.  The same handle returned
    /// by `load()` can be stored across frames and polled in subsequent
    /// `update()` callbacks.
    ///
    /// ```rust,ignore
    /// fn setup(&mut self, ctx: &mut AppContext) {
    ///     self.model = ctx.asset_server.load::<GltfModel>("assets/player.glb");
    /// }
    ///
    /// fn update(&mut self, ctx: &mut AppContext) {
    ///     if let AssetState::Ready(model) = ctx.asset_server.get(self.model) {
    ///         // spawn entities from model …
    ///     }
    /// }
    /// ```
    pub asset_server: &'a mut AssetServer,

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

    /// Switch the renderer's active shading style.
    ///
    /// This is a convenience shortcut for `ctx.render.set_style(style)`.
    ///
    /// ```rust,ignore
    /// ctx.set_render_style(RenderStyle::CelShaded { toon_levels: 4, outline_width: 0.02 });
    /// ```
    pub fn set_render_style(&mut self, style: ferrous_renderer::RenderStyle) {
        self.render.set_style(style);
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

    /// Move the OS window so that its top-left corner is at `(x, y)` in
    /// physical screen pixels (relative to the primary monitor's origin).
    ///
    /// This is a no-op on platforms where the OS does not allow programmatic
    /// window positioning (e.g. some Wayland compositors).  On those platforms
    /// the call is silently ignored.
    ///
    /// # Example — drag the window by a custom title bar
    /// ```rust,ignore
    /// fn update(&mut self, ctx: &mut AppContext) {
    ///     // `self.drag_offset` is set when the user presses on the title bar.
    ///     if ctx.input.button_held(MouseButton::Left) {
    ///         if let Some(offset) = self.drag_offset {
    ///             let (mx, my) = ctx.input.mouse_position();
    ///             ctx.set_window_position(mx as i32 - offset.0, my as i32 - offset.1);
    ///         }
    ///     } else {
    ///         self.drag_offset = None;
    ///     }
    /// }
    /// ```
    pub fn set_window_position(&self, x: i32, y: i32) {
        self.window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    /// Returns the current position of the window's top-left corner in
    /// physical screen pixels, or `None` if the platform does not support
    /// querying the window position.
    pub fn window_position(&self) -> Option<(i32, i32)> {
        self.window.outer_position().ok().map(|p| (p.x, p.y))
    }

    /// Begin an OS-native resize drag in the given direction.
    ///
    /// Call this on the frame the user **presses** the left mouse button while
    /// the cursor is over a resize handle you drew with `ferrous_gui`.  The OS
    /// then takes over the resize interaction until the button is released —
    /// you don't need to track mouse deltas yourself.
    ///
    /// This is a no-op on platforms that don't support it (e.g. some Wayland
    /// compositors).  The call is silently ignored in those cases.
    ///
    /// # Example
    /// ```rust,ignore
    /// use ferrous_app::WindowResizeDirection;
    ///
    /// fn update(&mut self, ctx: &mut AppContext) {
    ///     let (mx, my) = ctx.input.mouse_pos_f32();
    ///     let dir = resize_direction_for_pos(mx, my, ctx.width(), ctx.height());
    ///
    ///     if let Some(dir) = dir {
    ///         // Change cursor to a resize arrow
    ///         if ctx.input.button_just_pressed(MouseButton::Left) {
    ///             ctx.start_window_resize(dir);
    ///         }
    ///     }
    /// }
    ///
    /// fn resize_direction_for_pos(
    ///     mx: f32, my: f32, w: u32, h: u32,
    /// ) -> Option<WindowResizeDirection> {
    ///     const E: f32 = 8.0; // edge hit zone in pixels
    ///     let (w, h) = (w as f32, h as f32);
    ///     match (mx < E, mx > w - E, my < E, my > h - E) {
    ///         (true,  false, true,  false) => Some(WindowResizeDirection::NorthWest),
    ///         (false, true,  true,  false) => Some(WindowResizeDirection::NorthEast),
    ///         (true,  false, false, true)  => Some(WindowResizeDirection::SouthWest),
    ///         (false, true,  false, true)  => Some(WindowResizeDirection::SouthEast),
    ///         (true,  false, false, false) => Some(WindowResizeDirection::West),
    ///         (false, true,  false, false) => Some(WindowResizeDirection::East),
    ///         (false, false, true,  false) => Some(WindowResizeDirection::North),
    ///         (false, false, false, true)  => Some(WindowResizeDirection::South),
    ///         _ => None,
    ///     }
    /// }
    /// ```
    pub fn start_window_resize(&self, direction: ResizeDirection) {
        let _ = self.window.drag_resize_window(direction);
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
    /// the selected entity. Routes to translate or rotate interaction based
    /// on `gizmo.mode`.
    ///
    /// 1. Sync the gizmo transform to the entity's current world position.
    /// 2. Project handles into screen space using current camera matrices.
    /// 3. On left-click: pick the nearest handle within a 24 px threshold.
    /// 4. While dragging: apply the transform (translate or rotate around pivot).
    /// 5. Queue a [`GizmoDraw`] so the renderer draws the handles this frame.
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

        let arm_len = gizmo.style.arm_length;
        // Pivot point used for rotation gizmo origin and rotate-around target.
        let origin = gizmo.effective_pivot();

        match gizmo.mode {
            GizmoMode::Translate => {
                let plane_off = gizmo.style.plane_offset();
                let plane_size = gizmo.style.plane_size();

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
                        let mut sc = [(0.0_f32, 0.0_f32); 4];
                        for (i, &cw) in corners_world.iter().enumerate() {
                            match project(cw) {
                                Some(p) => sc[i] = p,
                                None => continue 'planes,
                            }
                        }
                        let mut quad_area = 0.0_f32;
                        for i in 0..4 {
                            let j = (i + 1) % 4;
                            quad_area += sc[i].0 * sc[j].1 - sc[j].0 * sc[i].1;
                        }
                        let sign = quad_area.signum();
                        let mut inside = true;
                        for i in 0..4 {
                            let j = (i + 1) % 4;
                            let cross = (sc[j].0 - sc[i].0) * (my - sc[i].1)
                                - (sc[j].1 - sc[i].1) * (mx - sc[i].0);
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
                    if let Some(plane) = gizmo.highlighted_plane {
                        let (a, b) = plane.axes();
                        let (dx, dy) = self.input.mouse_delta();
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
            }

            GizmoMode::Rotate => {
                // Pick: test proximity to each axis ring.
                // The ring for axis A lives in the plane perpendicular to A.
                // We approximate it as N screen points sampled around the circle
                // and find the minimum distance from the mouse to any segment.
                const RING_SEGS: usize = 32;
                let ring_pick_thresh = 18.0_f32;

                if self.input.button_just_pressed(MouseButton::Left) {
                    let mut best_axis: Option<Axis> = None;
                    let mut best_dist = ring_pick_thresh;

                    for &axis in &[Axis::X, Axis::Y, Axis::Z] {
                        let axis_vec = axis_vector(axis);
                        // Pick two stable perpendiculars in the ring plane.
                        let perp1 = if axis_vec.dot(Vec3::Y).abs() < 0.9 {
                            axis_vec.cross(Vec3::Y).normalize()
                        } else {
                            axis_vec.cross(Vec3::X).normalize()
                        };
                        let perp2 = axis_vec.cross(perp1).normalize();

                        // Sample ring points in world space, project to screen.
                        let mut ring_screen: Vec<(f32, f32)> = Vec::with_capacity(RING_SEGS);
                        let mut all_visible = true;
                        for i in 0..RING_SEGS {
                            let theta = (i as f32 / RING_SEGS as f32) * std::f32::consts::TAU;
                            let wp = origin + (perp1 * theta.cos() + perp2 * theta.sin()) * arm_len;
                            match project(wp) {
                                Some(sp) => ring_screen.push(sp),
                                None => {
                                    all_visible = false;
                                    break;
                                }
                            }
                        }
                        if !all_visible || ring_screen.is_empty() {
                            continue;
                        }

                        // Distance from mouse to ring polyline.
                        let n = ring_screen.len();
                        for i in 0..n {
                            let j = (i + 1) % n;
                            let (ax, ay) = ring_screen[i];
                            let (bx, by) = ring_screen[j];
                            let dx = bx - ax;
                            let dy = by - ay;
                            let len2 = dx * dx + dy * dy;
                            let dist = if len2 < 1e-4 {
                                let ex = mx - ax;
                                let ey = my - ay;
                                (ex * ex + ey * ey).sqrt()
                            } else {
                                let t = ((mx - ax) * dx + (my - ay) * dy) / len2;
                                let t = t.clamp(0.0, 1.0);
                                let cx = ax + t * dx - mx;
                                let cy = ay + t * dy - my;
                                (cx * cx + cy * cy).sqrt()
                            };
                            if dist < best_dist {
                                best_dist = dist;
                                best_axis = Some(axis);
                            }
                        }
                    }

                    gizmo.highlighted_axis = best_axis;
                    gizmo.highlighted_plane = None;
                    gizmo.dragging = best_axis.is_some();
                }

                if self.input.button_just_released(MouseButton::Left) {
                    gizmo.dragging = false;
                }

                // Drag: rotate entity around pivot by projecting mouse delta
                // onto the tangent of the ring at its "top" screen point.
                if gizmo.dragging {
                    if let Some(axis) = gizmo.highlighted_axis {
                        let axis_vec = axis_vector(axis);
                        // Find the tangent direction of the ring at the point
                        // closest to "right" on screen: rotate 90° around
                        // the axis to get a tangent.
                        let perp1 = if axis_vec.dot(Vec3::Y).abs() < 0.9 {
                            axis_vec.cross(Vec3::Y).normalize()
                        } else {
                            axis_vec.cross(Vec3::X).normalize()
                        };
                        // tangent = perp rotated 90° around axis = axis × perp1
                        let tangent = axis_vec.cross(perp1).normalize();
                        let tip = origin + tangent * arm_len;
                        if let (Some(os), Some(ts)) = (project(origin), project(tip)) {
                            let sx = ts.0 - os.0;
                            let sy = ts.1 - os.1;
                            let slen = (sx * sx + sy * sy).sqrt();
                            if slen > 1.0 {
                                let (dx, dy) = self.input.mouse_delta();
                                let screen_dot = (dx * sx + dy * sy) / slen;
                                // Map screen pixels to radians (scale by arm_len heuristic).
                                let angle = screen_dot / slen * std::f32::consts::PI;
                                let pivot = gizmo.effective_pivot();
                                self.world.rotate_around(handle, pivot, axis_vec, angle);
                            }
                        }
                    }
                }
            }

            GizmoMode::Scale => {
                // Scale mode: no handles yet; clear drag state so nothing fires.
                if self.input.button_just_released(MouseButton::Left) {
                    gizmo.dragging = false;
                }
            }
        }

        // Queue draw — position_matrix() strips entity scale so handles
        // are always the same size regardless of object dimensions.
        let mut draw = GizmoDraw::new(Mat4::from_translation(origin), gizmo.mode);
        draw.highlighted_axis = gizmo.highlighted_axis;
        draw.highlighted_plane = gizmo.highlighted_plane;
        draw.style = gizmo.style.clone();
        self.gizmos.push(draw);
    }

    /// Update a dedicated pivot-point gizmo.
    ///
    /// Call this from `draw_3d` alongside `update_gizmo` when you want to let
    /// the user move the rotation pivot independently.  The pivot gizmo always
    /// operates in `Translate` mode.  On each drag frame it adjusts
    /// `rotation_gizmo.pivot_offset` (local space) so that the pivot always
    /// follows the entity when it is translated.
    pub fn update_pivot_gizmo(
        &mut self,
        entity_handle: Handle,
        pivot_gizmo: &mut GizmoState,
        rotation_gizmo: &mut GizmoState,
    ) {
        // Keep pivot gizmo world-transform at the current pivot location.
        let pivot_pos = rotation_gizmo.effective_pivot();
        if let Some(tr) = self.world.transform(entity_handle) {
            let mut pivot_tr = tr;
            pivot_tr.position = pivot_pos;
            pivot_gizmo.update_world_transform(pivot_tr);
        }

        let (win_w, win_h) = (self.window_size.0 as f32, self.window_size.1 as f32);
        let (mx, my) = {
            let (px, py) = self.input.mouse_position();
            (px as f32, py as f32)
        };

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

        let arm_len = pivot_gizmo.style.arm_length;
        let origin = pivot_gizmo.world_transform.position;

        // Axis picking (translate only, no planes for pivot gizmo).
        if self.input.button_just_pressed(MouseButton::Left) {
            let mut best_axis: Option<Axis> = None;
            let mut best_dist = 24.0_f32;
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
                    if dist < best_dist {
                        best_dist = dist;
                        best_axis = Some(axis);
                    }
                }
            }
            pivot_gizmo.highlighted_axis = best_axis;
            pivot_gizmo.highlighted_plane = None;
            pivot_gizmo.dragging = best_axis.is_some();
        }

        if self.input.button_just_released(MouseButton::Left) {
            pivot_gizmo.dragging = false;
        }

        if pivot_gizmo.dragging {
            if let Some(axis) = pivot_gizmo.highlighted_axis {
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
                        let offset_w = av * world_delta;

                        // Move the pivot offset in local space.
                        let local_delta =
                            rotation_gizmo.world_transform.rotation.inverse() * offset_w;
                        rotation_gizmo.pivot_offset += local_delta;

                        pivot_gizmo.world_transform.position += offset_w;
                    }
                }
            }
        }

        // Queue draw for pivot gizmo.
        let mut draw = GizmoDraw::new(Mat4::from_translation(origin), GizmoMode::Translate);
        draw.highlighted_axis = pivot_gizmo.highlighted_axis;
        draw.highlighted_plane = None;
        draw.style = pivot_gizmo.style.clone();
        self.gizmos.push(draw);
    }
}
