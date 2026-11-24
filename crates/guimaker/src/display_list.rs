//! The engine-agnostic display list.
//!
//! Every command names no engine type. `guimaker_ferrous` maps each variant
//! onto `GuiBatch`; the core only says what to draw and to which node it
//! belongs (the attribution prompt 19's debugger reads).

use std::sync::Arc;

use glam::Vec2;

use crate::geom::Rect;
use crate::text::{Line, TextStyle};
use crate::theme::Color;
use crate::view::NodeId;

/// How an image fills its rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    /// Crop to cover the rect.
    Cover,
    /// Letterbox inside the rect.
    Contain,
    /// Stretch to the rect.
    Fill,
}

/// One gradient stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position in the gradient, `0.0..=1.0`.
    pub offset: f32,
    /// The stop colour.
    pub color: Color,
}

/// A gradient description.
#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    /// A linear gradient at `angle` degrees.
    Linear {
        /// Direction in degrees.
        angle: f32,
        /// The stops, in order.
        stops: Vec<GradientStop>,
    },
    /// A radial gradient centred in the rect.
    Radial {
        /// Radius in logical pixels.
        radius: f32,
        /// The stops, in order.
        stops: Vec<GradientStop>,
    },
}

/// A text span's resolved emphasis, used by the adapter to colour a shaped run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSpanStyle {
    /// Byte start in the command's text.
    pub start: usize,
    /// Byte end (exclusive).
    pub end: usize,
    /// The span's colour.
    pub color: Color,
    /// Bold emphasis.
    pub strong: bool,
    /// Monospace emphasis.
    pub mono: bool,
    /// Link emphasis.
    pub link: bool,
}

/// The shaped lines of a text run, shared with the adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    /// The full run text.
    pub text: Arc<str>,
    /// The shaped line boxes.
    pub lines: Arc<[Line]>,
    /// The base text style.
    pub style: TextStyle,
    /// The span colours and emphasis.
    pub spans: Arc<[TextSpanStyle]>,
}

/// One draw command.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    /// A solid rounded rectangle.
    Rect {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// The fill colour.
        color: Color,
        /// Per-corner radii (top-left, top-right, bottom-right, bottom-left).
        radii: [f32; 4],
    },
    /// A rounded border.
    Border {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// The border colour.
        color: Color,
        /// Border width in logical pixels.
        width: f32,
        /// Per-edge border widths in logical pixels: top, right, bottom, left.
        sides: [f32; 4],
        /// Per-corner radii.
        radii: [f32; 4],
    },
    /// One shadow layer.
    Shadow {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// The shadow colour.
        color: Color,
        /// Offset.
        offset: Vec2,
        /// Blur radius.
        blur: f32,
        /// Spread.
        spread: f32,
        /// Per-corner radii.
        radii: [f32; 4],
    },
    /// A gradient fill.
    Gradient {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// The gradient.
        gradient: Gradient,
        /// Per-corner radii.
        radii: [f32; 4],
    },
    /// A shaped text run.
    Text {
        /// The owning node.
        node: NodeId,
        /// The text box.
        rect: Rect,
        /// The shaped run.
        run: TextRun,
    },
    /// An image.
    Image {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// The adapter-defined source id.
        source: u64,
        /// The fit mode.
        fit: ImageFit,
        /// Per-corner radii.
        radii: [f32; 4],
    },
    /// A vector icon path.
    Icon {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// The path data, owned by the adapter's icon set.
        path: Arc<str>,
        /// The icon colour.
        color: Color,
    },
    /// A stroked polyline in absolute pixels.
    ///
    /// The adapter draws it as connected round-capped line segments through
    /// the engine path stroke (`GuiBatch::polyline`), so an arc stays smooth at
    /// any support and both ends keep a half-disc cap.
    Path {
        /// The owning node.
        node: NodeId,
        /// The points, in paint order.
        points: Arc<[Vec2]>,
        /// The stroke width in physical pixels.
        width: f32,
        /// The stroke colour.
        color: Color,
    },
    /// Pushes a clip rectangle.
    ClipPush {
        /// The owning node.
        node: NodeId,
        /// The clip rectangle.
        rect: Rect,
        /// Per-corner radii.
        radii: [f32; 4],
    },
    /// Pops the last clip.
    ClipPop {
        /// The owning node.
        node: NodeId,
    },
    /// Starts an opacity layer.
    LayerPush {
        /// The owning node.
        node: NodeId,
        /// The layer opacity.
        opacity: f32,
    },
    /// Ends an opacity layer.
    LayerPop {
        /// The owning node.
        node: NodeId,
    },
    /// Requests a backdrop blur behind the rectangle.
    BackdropBlur {
        /// The owning node.
        node: NodeId,
        /// The rectangle.
        rect: Rect,
        /// Blur radius in logical pixels.
        radius: f32,
        /// Per-corner radii.
        radii: [f32; 4],
    },
}

/// A flat display list with per-node attribution.
#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    /// The commands, in paint order.
    pub commands: Vec<DrawCommand>,
    /// This frame's damage.
    pub damage: Vec<Rect>,
}

impl DisplayList {
    /// Builds an empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the commands and damage, keeping capacity.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.damage.clear();
    }

    /// The commands a node produced (a debug-only scan).
    #[must_use]
    pub fn commands_for(&self, node: NodeId) -> Vec<&DrawCommand> {
        self.commands
            .iter()
            .filter(|command| command.node() == node)
            .collect()
    }

    /// The union of this frame's damage.
    #[must_use]
    pub fn damage_union(&self) -> Option<Rect> {
        crate::layout::damage_union(&self.damage)
    }
}

impl DrawCommand {
    /// Moves every geometry the command carries by `(dx, dy)` logical pixels.
    ///
    /// A host uses it to offset an overlay's window coordinates to the origin
    /// of the popup window that draws it.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Self::Rect { rect, .. }
            | Self::Border { rect, .. }
            | Self::Shadow { rect, .. }
            | Self::Gradient { rect, .. }
            | Self::Text { rect, .. }
            | Self::Image { rect, .. }
            | Self::Icon { rect, .. }
            | Self::ClipPush { rect, .. }
            | Self::BackdropBlur { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
            Self::Path { points, .. } => {
                *points = points
                    .iter()
                    .map(|point| Vec2::new(point.x + dx, point.y + dy))
                    .collect();
            }
            Self::ClipPop { .. } | Self::LayerPush { .. } | Self::LayerPop { .. } => {}
        }
    }

    /// The node this command belongs to.
    #[must_use]
    pub fn node(&self) -> NodeId {
        match self {
            Self::Rect { node, .. }
            | Self::Border { node, .. }
            | Self::Shadow { node, .. }
            | Self::Gradient { node, .. }
            | Self::Text { node, .. }
            | Self::Image { node, .. }
            | Self::Icon { node, .. }
            | Self::Path { node, .. }
            | Self::ClipPush { node, .. }
            | Self::ClipPop { node }
            | Self::LayerPush { node, .. }
            | Self::LayerPop { node }
            | Self::BackdropBlur { node, .. } => *node,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DisplayList;
    use crate::geom::Rect;
    use crate::view::NodeId;

    #[test]
    fn commands_attribute_to_nodes() {
        let mut list = DisplayList::new();
        list.commands
            .push(super::DrawCommand::ClipPop { node: NodeId(1) });
        assert_eq!(list.commands_for(NodeId(1)).len(), 1);
        assert!(list.commands_for(NodeId(2)).is_empty());
        assert_eq!(list.commands[0].node(), NodeId(1));
        list.damage.push(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(list.damage_union(), Some(Rect::new(0.0, 0.0, 10.0, 10.0)));
    }
}
