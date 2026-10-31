use glam::Vec2;

// ─── Memory Safety Constants ───
/// Maximum number of commands allowed in a single path to prevent buffer overflow
const MAX_PATH_COMMANDS: usize = 10_000;
/// Pre-allocated capacity for common path sizes (typically 50-100 commands)
const PATH_BUILDER_CAPACITY: usize = 256;

/// Style used to fill the corner between two connected path segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinStyle {
    /// Extend the outer edges until they meet (sharp corners).
    Miter,
    /// Cut the corner with a straight edge.
    Bevel,
    /// Round the corner with an arc.
    Round,
}

/// Style used to terminate the open ends of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapStyle {
    /// End exactly on the endpoint.
    Flat,
    /// End with a half-disc of radius `stroke_width / 2`.
    Round,
    /// End with a square that extends `stroke_width / 2` past the endpoint.
    Square,
}

/// A single instruction in a polyline path.
#[derive(Debug, Clone)]
pub enum PathCommand {
    /// Start a new subpath at the given position.
    MoveTo(Vec2),
    /// Append a straight segment to the given position.
    LineTo(Vec2),
}

/// Fluent builder for a [`Path2d`] polyline.
#[derive(Debug, Clone)]
pub struct PathBuilder {
    commands: Vec<PathCommand>,
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PathBuilder {
    /// Create an empty builder with the default pre-allocated capacity.
    pub fn new() -> Self {
        let commands = Vec::with_capacity(PATH_BUILDER_CAPACITY);
        Self { commands }
    }

    /// Create an empty builder reserving room for `capacity` commands
    /// (clamped to the internal `MAX_PATH_COMMANDS` limit).
    pub fn with_capacity(capacity: usize) -> Self {
        let commands = Vec::with_capacity(capacity.min(MAX_PATH_COMMANDS));
        Self { commands }
    }

    /// Append a `MoveTo` command, starting a new subpath.
    ///
    /// Commands are silently dropped once the internal `MAX_PATH_COMMANDS` limit is reached.
    pub fn move_to(mut self, pos: Vec2) -> Self {
        if self.commands.len() < MAX_PATH_COMMANDS {
            self.commands.push(PathCommand::MoveTo(pos));
        }
        self
    }

    /// Append a `LineTo` command, extending the current subpath.
    ///
    /// Commands are silently dropped once the internal `MAX_PATH_COMMANDS` limit is reached.
    pub fn line_to(mut self, pos: Vec2) -> Self {
        if self.commands.len() < MAX_PATH_COMMANDS {
            self.commands.push(PathCommand::LineTo(pos));
        }
        self
    }

    /// Currently a no-op retained for builder-chain compatibility.
    ///
    /// `build` always produces an open path; to close it, set `Path2d::is_closed`
    /// or call [`Path2d::closed`] on the built value.
    pub fn close(self) -> Self {
        // Un helper para poder invocarlo si se necesita cerrar explicitamente
        self
    }

    /// Finish the builder and produce an open [`Path2d`] with default stroke
    /// (`white`, width `1.0`, `Miter` join, `Flat` cap, z `0.0`).
    pub fn build(self) -> Path2d {
        Path2d {
            commands: self.commands,
            stroke_width: 1.0,
            stroke_color: [1.0, 1.0, 1.0, 1.0],
            fill_color: [0.0; 4],
            join_style: JoinStyle::Miter,
            cap_style: CapStyle::Flat,
            is_filled: false,
            is_closed: false,
            z_index: 0.0,
        }
    }
}

/// Component for rendering robust 2D paths (lines, polygons, curves) with CAD-level precision.
#[derive(Debug, Clone)]
pub struct Path2d {
    /// Ordered list of subpath instructions.
    pub commands: Vec<PathCommand>,

    /// Stroke width in world units.
    pub stroke_width: f32,
    /// Stroke colour (RGBA).
    pub stroke_color: [f32; 4],

    /// Whether the path interior should be filled.
    pub is_filled: bool,
    /// Fill colour (RGBA); used when `is_filled` is `true`.
    pub fill_color: [f32; 4],

    /// Join style for corners shared between segments. Read by
    /// `prepare_paths_system`: `Round`/`Bevel` use a true round-join disc
    /// (`stroke_width/2`), `Miter` grows the disc with the miter ratio on
    /// sharp corners. Exact miter/bevel tessellation is a future improvement.
    pub join_style: JoinStyle,
    /// Cap style applied to the open ends of each subpath.
    pub cap_style: CapStyle,
    /// Whether the final segment connects back to the subpath start.
    pub is_closed: bool,
    /// Depth used for z-sorting; larger values draw in front.
    pub z_index: f32,
}

impl Path2d {
    /// Set the stroke colour and width, returning `self` for chaining.
    pub fn with_stroke(mut self, color: [f32; 4], width: f32) -> Self {
        self.stroke_color = color;
        self.stroke_width = width;
        self
    }

    /// Enable filling with the given colour, returning `self` for chaining.
    pub fn with_fill(mut self, color: [f32; 4]) -> Self {
        self.is_filled = true;
        self.fill_color = color;
        self
    }

    /// Set the corner [`JoinStyle`], returning `self` for chaining.
    pub fn with_join(mut self, style: JoinStyle) -> Self {
        self.join_style = style;
        self
    }

    /// Set the end [`CapStyle`], returning `self` for chaining.
    pub fn with_cap(mut self, style: CapStyle) -> Self {
        self.cap_style = style;
        self
    }

    /// Mark the path as closed, returning `self` for chaining.
    pub fn closed(mut self) -> Self {
        self.is_closed = true;
        self
    }
}

impl ferrous_ecs::component::Component for Path2d {}
