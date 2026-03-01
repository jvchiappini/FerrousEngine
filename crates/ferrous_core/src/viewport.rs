/// Rectangular region within the render target used for 3-D rendering.
///
/// Set `ctx.viewport` in your `update()` callback to control which area of
/// the window the 3-D scene is rendered into.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
