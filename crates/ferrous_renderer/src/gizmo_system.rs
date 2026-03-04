//! `GizmoSystem` — owns the gizmo GPU pipeline and per-frame draw queue.
//!
//! Extracted from `Renderer` so that all gizmo-related state and logic live
//! in one focused module.  The rest of the renderer calls two methods:
//!
//! * [`GizmoSystem::queue`] — push a [`GizmoDraw`] for the current frame.
//! * [`GizmoSystem::execute`] — bake a vertex buffer and record a render pass.

use std::sync::Arc;

use crate::geometry::Vertex;
use crate::pipeline::{GizmoPipeline, PipelineLayouts};
use crate::scene::GizmoDraw;

// --------------------------------------------------------------------------
// GizmoSystem
// --------------------------------------------------------------------------

/// Manages editor gizmo rendering for a single frame.
///
/// Owns the line-list GPU pipeline and the per-frame list of gizmos to draw.
/// Call [`queue`](GizmoSystem::queue) from application code and
/// [`execute`](GizmoSystem::execute) once per frame from `Renderer::do_render`.
pub struct GizmoSystem {
    /// GPU pipeline: a `wgpu::RenderPipeline` configured for `LineList`
    /// topology using the gizmo WGSL shader.
    pub pipeline: GizmoPipeline,
    /// Gizmos queued for the current frame; cleared after each `execute`.
    pub draws: Vec<GizmoDraw>,
}

impl GizmoSystem {
    /// Create the gizmo system, compiling the GPU pipeline once.
    pub fn new(
        device: &wgpu::Device,
        hdr_format: wgpu::TextureFormat,
        sample_count: u32,
        layouts: PipelineLayouts,
    ) -> Self {
        Self {
            pipeline: GizmoPipeline::new(device, hdr_format, sample_count, layouts),
            draws: Vec::new(),
        }
    }

    /// Queue a gizmo to be rendered this frame.
    ///
    /// The gizmo list is cleared at the end of [`execute`](Self::execute),
    /// so each `queue` call is valid only for the current frame.
    #[inline]
    pub fn queue(&mut self, gizmo: GizmoDraw) {
        self.draws.push(gizmo);
    }

    /// Returns `true` if there are any gizmos queued this frame.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }

    /// Bake vertex data for all queued gizmos, upload a transient vertex
    /// buffer, and record a `LineList` render pass into `encoder`.
    ///
    /// The pass writes into `hdr_view` (the world-pass HDR texture) using
    /// `LoadOp::Load` so that gizmos composite correctly over the 3-D scene.
    /// Depth is also loaded so gizmos respect scene occlusion.
    ///
    /// `camera_bind_group` is the bind group for the camera uniform (group 0
    /// in the gizmo shader).
    ///
    /// After the pass `draws` is cleared, ready for the next frame.
    pub fn execute(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        hdr_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera_bind_group: &Arc<wgpu::BindGroup>,
    ) {
        use wgpu::util::DeviceExt;
        use wgpu::{
            LoadOp, Operations, RenderPassColorAttachment, RenderPassDepthStencilAttachment,
            RenderPassDescriptor, StoreOp,
        };

        // -- Build vertex list: each line segment = two consecutive vertices --
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut push_line = |pos: [f32; 3], col: [f32; 3]| {
            let mut vert = Vertex::new(pos, [0.0, 0.0, 1.0], [0.0, 0.0]);
            vert.color = col;
            vertices.push(vert);
        };

        for gizmo in &self.draws {
            use ferrous_core::scene::{GizmoMode, Plane};
            let st = &gizmo.style;

            let arm = st.arm_length;
            let p_off = st.plane_offset();
            let p_size = st.plane_size();
            let arr_len = st.arrow_length();
            let arr_half = st.arrow_half_angle_deg.to_radians();

            let m = gizmo.transform;

            match gizmo.mode {
                GizmoMode::Translate | GizmoMode::Scale => {
                    // -- Axis arms + optional arrowheads ----------------------
                    for &(axis_vec, axis_enum) in &[
                        (glam::Vec3::X, ferrous_core::scene::Axis::X),
                        (glam::Vec3::Y, ferrous_core::scene::Axis::Y),
                        (glam::Vec3::Z, ferrous_core::scene::Axis::Z),
                    ] {
                        let c = if gizmo.highlighted_axis == Some(axis_enum) {
                            st.axis_highlight(axis_enum)
                        } else {
                            st.axis_color(axis_enum)
                        };

                        let p0 = m.transform_point3(glam::Vec3::ZERO);
                        let p1 = m.transform_point3(axis_vec * arm);

                        push_line(p0.into(), c);
                        push_line(p1.into(), c);

                        if st.show_arrows && arr_len > 1e-4 {
                            let perp = if axis_vec.dot(glam::Vec3::Y).abs() < 0.9 {
                                axis_vec.cross(glam::Vec3::Y).normalize()
                            } else {
                                axis_vec.cross(glam::Vec3::X).normalize()
                            };
                            let base_local = axis_vec * (arm - arr_len);
                            let up2 = perp;
                            let side = axis_vec.cross(perp).normalize();
                            for &fin_dir in &[up2, -up2, side, -side] {
                                let fin_tip = axis_vec * arm;
                                let fin_base = base_local + fin_dir * (arr_len * arr_half.tan());
                                push_line(m.transform_point3(fin_tip).into(), c);
                                push_line(m.transform_point3(fin_base).into(), c);
                            }
                        }
                    }

                    // -- Plane square outlines ---------------------------------
                    if st.show_planes {
                        for &plane in &[Plane::XY, Plane::XZ, Plane::YZ] {
                            let rgba = if gizmo.highlighted_plane == Some(plane) {
                                st.plane_highlight(plane)
                            } else {
                                st.plane_color(plane)
                            };
                            let c = [rgba[0], rgba[1], rgba[2]];
                            let (a, b) = plane.axes();
                            let c0 = a * p_off + b * p_off;
                            let c1 = a * (p_off + p_size) + b * p_off;
                            let c2 = a * (p_off + p_size) + b * (p_off + p_size);
                            let c3 = a * p_off + b * (p_off + p_size);
                            let corners = [
                                m.transform_point3(c0),
                                m.transform_point3(c1),
                                m.transform_point3(c2),
                                m.transform_point3(c3),
                            ];
                            for i in 0..4 {
                                let j = (i + 1) % 4;
                                push_line(corners[i].into(), c);
                                push_line(corners[j].into(), c);
                            }
                        }
                    }
                }

                GizmoMode::Rotate => {
                    // -- Rotation arc rings — one full circle per axis --------
                    const ARC_SEGS: usize = 48;
                    let origin = m.transform_point3(glam::Vec3::ZERO);

                    for &(axis_vec, axis_enum) in &[
                        (glam::Vec3::X, ferrous_core::scene::Axis::X),
                        (glam::Vec3::Y, ferrous_core::scene::Axis::Y),
                        (glam::Vec3::Z, ferrous_core::scene::Axis::Z),
                    ] {
                        let c = if gizmo.highlighted_axis == Some(axis_enum) {
                            st.axis_highlight(axis_enum)
                        } else {
                            st.axis_color(axis_enum)
                        };

                        let perp1 = if axis_vec.dot(glam::Vec3::Y).abs() < 0.9 {
                            axis_vec.cross(glam::Vec3::Y).normalize()
                        } else {
                            axis_vec.cross(glam::Vec3::X).normalize()
                        };
                        let perp2 = axis_vec.cross(perp1).normalize();

                        let mut ring: Vec<[f32; 3]> = Vec::with_capacity(ARC_SEGS);
                        for i in 0..ARC_SEGS {
                            let theta = (i as f32 / ARC_SEGS as f32) * std::f32::consts::TAU;
                            let local = (perp1 * theta.cos() + perp2 * theta.sin()) * arm;
                            ring.push((origin + local).into());
                        }

                        for i in 0..ARC_SEGS {
                            let j = (i + 1) % ARC_SEGS;
                            push_line(ring[i], c);
                            push_line(ring[j], c);
                        }
                    }

                    // Small pivot dot (cross) so the user can see the origin.
                    let dot_size = arm * 0.06;
                    let pivot_c = [1.0_f32, 1.0, 0.4];
                    for &dir in &[
                        glam::Vec3::X,
                        glam::Vec3::NEG_X,
                        glam::Vec3::Y,
                        glam::Vec3::NEG_Y,
                        glam::Vec3::Z,
                        glam::Vec3::NEG_Z,
                    ] {
                        push_line(origin.into(), pivot_c);
                        push_line((origin + dir * dot_size).into(), pivot_c);
                    }
                }
            }
        }

        // -- Upload transient vertex buffer -----------------------------------
        if vertices.is_empty() {
            return;
        }

        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gizmo vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // -- Record the render pass -------------------------------------------
        // Writes into the HDR texture (same target as WorldPass) so gizmos are
        // tone-mapped with the rest of the scene.
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Gizmo Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: hdr_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&self.pipeline.inner);
        rpass.set_bind_group(0, &**camera_bind_group, &[]);
        rpass.set_vertex_buffer(0, vb.slice(..));
        let vertex_count = vertices.len() as u32;
        if vertex_count > 0 {
            rpass.draw(0..vertex_count, 0..1);
        }

        // Clear for the next frame.
        self.draws.clear();
    }
}
