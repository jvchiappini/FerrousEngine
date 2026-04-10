use serde::{Deserialize, Serialize};

/// Axis-aligned rectangle defined by its origin (top-left) and dimensions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Calculates the intersection of two rectangles.
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);

        Rect {
            x,
            y,
            width: (x2 - x).max(0.0),
            height: (y2 - y).max(0.0),
        }
    }

    /// Calculates the union of two rectangles (smallest rectangle containing both).
    pub fn union(&self, other: Rect) -> Rect {
        if self.width <= 0.0 && self.height <= 0.0 { return other; }
        if other.width <= 0.0 && other.height <= 0.0 { return *self; }

        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);

        Rect {
            x,
            y,
            width: x2 - x,
            height: y2 - y,
        }
    }

    /// Checks if this rectangle overlaps with another.
    pub fn intersects(&self, other: &Rect) -> bool {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);

        x2 > x && y2 > y
    }

    /// Checks if a point is inside the rectangle.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x
            && p[0] <= self.x + self.width
            && p[1] >= self.y
            && p[1] <= self.y + self.height
    }
}

/// Offsets for the four sides of a rectangle.
/// Used for margins and paddings.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RectOffset {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl RectOffset {
    /// Creates a uniform offset for all sides.
    pub fn all(v: f32) -> Self {
        Self {
            left: v,
            right: v,
            top: v,
            bottom: v,
        }
    }
}

/// Measurement units for the layout system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Units {
    /// Absolute value in physical pixels.
    Px(f32),
    /// Relative value to parent container size (0.0 to 100.0).
    Percentage(f32),
    /// Flex unit for distributing space in Flexbox layouts.
    Flex(f32),
    /// Automatically adjusts to content or container size.
    Auto,
}

impl Default for Units {
    fn default() -> Self {
        Units::Px(0.0)
    }
}

/// Alignment of elements within their container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    Start,
    Center,
    End,
    Stretch,
}

impl Default for Alignment {
    fn default() -> Self {
        Alignment::Start
    }
}

/// Child layout behavior and node positioning logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    /// Standard block behavior (stacked or absolute).
    Block,
    /// Horizontal flex layout.
    FlexRow,
    /// Vertical flex layout.
    FlexColumn,
}

impl Default for DisplayMode {
    fn default() -> Self {
        DisplayMode::Block
    }
}

/// How the node is positioned relative to siblings and parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    /// Relative to normal layout flow.
    Relative,
    /// Absolute positioning relative to parent, ignoring siblings.
    Absolute,
}

impl Default for Position {
    fn default() -> Self {
        Position::Relative
    }
}

/// Horizontal text alignment within a bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HAlign {
    Left,
    Center,
    Right,
    Custom {
        value: f32,
        percent: bool,
        pivot: f32,
    },
}

impl Default for HAlign {
    fn default() -> Self {
        HAlign::Center
    }
}

/// Vertical text alignment within a bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
    Custom {
        value: f32,
        percent: bool,
        pivot: f32,
    },
}

impl Default for VAlign {
    fn default() -> Self {
        VAlign::Center
    }
}

/// Combined horizontal and vertical text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TextAlign {
    pub h: HAlign,
    pub v: VAlign,
}

impl TextAlign {
    pub const fn new(h: HAlign, v: VAlign) -> Self {
        Self { h, v }
    }

    pub const CENTER: Self = Self {
        h: HAlign::Center,
        v: VAlign::Center,
    };

    pub const TOP_LEFT: Self = Self {
        h: HAlign::Left,
        v: VAlign::Top,
    };

    pub fn resolve_x(self, rect_x: f32, rect_w: f32, text_w: f32, pad: f32) -> f32 {
        match self.h {
            HAlign::Left => rect_x + pad,
            HAlign::Right => rect_x + rect_w - text_w - pad,
            HAlign::Center => rect_x + (rect_w - text_w) * 0.5,
            HAlign::Custom {
                value,
                percent,
                pivot,
            } => {
                let anchor = if percent {
                    rect_x + rect_w * (value / 100.0)
                } else {
                    rect_x + value
                };
                anchor - text_w * pivot
            }
        }
    }

    pub fn resolve_y(self, rect_y: f32, rect_h: f32, text_h: f32, pad: f32) -> f32 {
        match self.v {
            VAlign::Top => rect_y + pad,
            VAlign::Bottom => rect_y + rect_h - text_h - pad,
            VAlign::Center => rect_y + (rect_h - text_h) * 0.5,
            VAlign::Custom {
                value,
                percent,
                pivot,
            } => {
                let anchor = if percent {
                    rect_y + rect_h * (value / 100.0)
                } else {
                    rect_y + value
                };
                anchor - text_h * pivot
            }
        }
    }
}

/// Content behavior when exceeding node dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
}

impl Default for Overflow {
    fn default() -> Self {
        Overflow::Visible
    }
}

/// Container for visual and positioning properties of a Widget.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Style {
    pub margin: RectOffset,
    pub padding: RectOffset,
    pub size: (Units, Units),
    pub alignment: Alignment,
    pub display: DisplayMode,
    pub position: Position,
    pub offsets: RectOffset,
    pub overflow: Overflow,
    pub gap: f32,
}