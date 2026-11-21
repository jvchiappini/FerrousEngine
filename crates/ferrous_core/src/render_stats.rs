/// Per-frame renderer statistics exposed to the application layer.
///
/// Accessible via `ctx.render_stats` inside any `FerrousApp` callback.
///
/// ## Example
/// ```rust,ignore
/// fn draw_ui(&mut self, _gui: &mut GuiBatch, text: &mut TextBatch,
///             font: Option<&Font>, ctx: &mut AppContext) {
///     if let Some(f) = font {
///         let s = ctx.render_stats;
///         text.draw_text(f, &format!("Tris: {}", s.triangle_count), …);
///     }
/// }
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct RenderStats {
    /// Total vertices submitted this frame (after frustum culling).
    pub vertex_count: u64,
    /// Total triangles submitted this frame.
    pub triangle_count: u64,
    /// Number of GPU draw calls issued this frame.
    ///
    /// 3-D draw calls come from the frame builder; the 2-D and GUI passes add
    /// theirs before the frame ends (prompt 18 P5).
    pub draw_calls: u32,
    /// Render passes opened for the 2-D and GUI content this frame.
    pub gpu_passes: u32,
    /// Raw `set_pipeline` calls issued by the 2-D and GUI renderers.
    pub pipeline_binds: u32,
    /// Raw `set_bind_group` calls issued by the 2-D and GUI renderers.
    pub bind_group_binds: u32,
    /// Raw `set_vertex_buffer` + `set_index_buffer` calls.
    pub buffer_binds: u32,
    /// `set_scissor_rect` calls issued by the GUI renderer.
    pub scissor_changes: u32,
    /// GUI draw segments recorded this frame.
    pub ui_segments: u32,
}
