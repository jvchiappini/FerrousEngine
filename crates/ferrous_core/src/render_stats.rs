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
    pub draw_calls: u32,
}
