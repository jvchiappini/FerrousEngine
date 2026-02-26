//! Path utilities used by the font parser and MSDF generator.

/// A simplified representation of drawing commands for a glyph.  Coordinates
/// are normalized (divide by units per em) and y increases upward.
#[derive(Debug, PartialEq, Clone)]
pub enum GlyphCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo {
        ctrl_x: f32,
        ctrl_y: f32,
        to_x: f32,
        to_y: f32,
    },
}

/// A glyph outline is just a sequence of path commands.
pub type GlyphOutline = Vec<GlyphCommand>;
