use glam::Vec2;

// ─── Memory Safety Constants ───
/// Maximum number of commands allowed in a single path to prevent buffer overflow
const MAX_PATH_COMMANDS: usize = 10_000;
/// Pre-allocated capacity for common path sizes (typically 50-100 commands)
const PATH_BUILDER_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinStyle {
    Miter,
    Bevel,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapStyle {
    Flat,
    Round,
    Square,
}

#[derive(Debug, Clone)]
pub enum PathCommand {
    MoveTo(Vec2),
    LineTo(Vec2),
}

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
    pub fn new() -> Self {
        let mut commands = Vec::new();
        commands.reserve(PATH_BUILDER_CAPACITY);
        Self { commands }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let mut commands = Vec::new();
        commands.reserve(capacity.min(MAX_PATH_COMMANDS));
        Self { commands }
    }

    pub fn move_to(mut self, pos: Vec2) -> Self {
        if self.commands.len() < MAX_PATH_COMMANDS {
            self.commands.push(PathCommand::MoveTo(pos));
        }
        self
    }

    pub fn line_to(mut self, pos: Vec2) -> Self {
        if self.commands.len() < MAX_PATH_COMMANDS {
            self.commands.push(PathCommand::LineTo(pos));
        }
        self
    }
    
    pub fn close(self) -> Self {
        // Un helper para poder invocarlo si se necesita cerrar explicitamente 
        self
    }

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
        }
    }
}

/// Component for rendering robust 2D paths (lines, polygons, curves) with CAD-level precision.
#[derive(Debug, Clone)]
pub struct Path2d {
    pub commands: Vec<PathCommand>,
    
    pub stroke_width: f32,
    pub stroke_color: [f32; 4],
    
    pub is_filled: bool,
    pub fill_color: [f32; 4],
    
    pub join_style: JoinStyle,
    pub cap_style: CapStyle,
    pub is_closed: bool,
}

impl Path2d {
    pub fn with_stroke(mut self, color: [f32;4], width: f32) -> Self {
        self.stroke_color = color;
        self.stroke_width = width;
        self
    }

    pub fn with_fill(mut self, color: [f32;4]) -> Self {
        self.is_filled = true;
        self.fill_color = color;
        self
    }

    pub fn with_join(mut self, style: JoinStyle) -> Self {
        self.join_style = style;
        self
    }

    pub fn with_cap(mut self, style: CapStyle) -> Self {
        self.cap_style = style;
        self
    }
    
    pub fn closed(mut self) -> Self {
        self.is_closed = true;
        self
    }
}

impl ferrous_ecs::component::Component for Path2d {}
