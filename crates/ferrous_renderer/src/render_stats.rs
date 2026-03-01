/// Per-frame renderer statistics.
///
/// Updated every frame by `Renderer::build_base_packet` and accessible via
/// [`Renderer::render_stats`].
///
/// ## Example (in `draw_ui`)
/// ```rust
/// let stats = ctx.renderer().render_stats();
/// // draw stats in the UI overlay...
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct RenderStats {
    /// Total number of vertices submitted this frame (visible geometry only,
    /// after frustum culling).  Instanced objects are counted once per
    /// instance (`vertex_count_per_mesh × instance_count`).
    pub vertex_count: u64,
    /// Total number of triangles submitted this frame (= `index_count / 3` per
    /// draw, summed across all draw calls, multiplied by instance count).
    pub triangle_count: u64,
    /// Number of GPU draw calls issued this frame (one per `DrawCommand` +
    /// one per `InstancedDrawCommand`).
    pub draw_calls: u32,
}
