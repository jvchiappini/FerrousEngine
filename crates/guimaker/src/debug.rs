//! Debug data sources designed for prompt 19.
//!
//! A frame exposes plain data: every node's stable id and path, its rectangle,
//! its resolved tokens, whether layout and paint were reused, why it repainted,
//! and the range of display commands attributed to it. Prompt 19 only adds the
//! panels; the data already exists and is deterministic.

use crate::geom::{Rect, Size};
use crate::layout::LayoutStats;
use crate::store::RepaintReason;
use crate::style::ResolvedStyle;
use crate::theme::{Density, ThemeMode};
use crate::view::NodeId;

/// Whether a node's layout, text and paint were reused this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheState {
    /// The node's whole subtree was unchanged; taffy was not touched.
    pub layout_reused: bool,
    /// The shaped text was reused.
    pub text_reused: bool,
    /// The cached draw commands were replayed.
    pub paint_reused: bool,
}

/// One node's debug record.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDebug {
    /// The node id.
    pub id: NodeId,
    /// The stable path (`start/actions/open`).
    pub path: String,
    /// The structural kind.
    pub kind: &'static str,
    /// The laid-out rectangle.
    pub rect: Rect,
    /// The resolved tokens and colours.
    pub resolved: ResolvedStyle,
    /// Layout/text/paint reuse.
    pub cache: CacheState,
    /// Why the node repainted.
    pub repaint: RepaintReason,
    /// How many display commands the node produced this frame.
    pub commands: usize,
    /// The text of a text node.
    pub text: Option<String>,
}

/// Everything prompt 19's widget reflector needs for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameDebug {
    /// The viewport in logical pixels.
    pub viewport: Size,
    /// The host scale factor.
    pub scale: f32,
    /// The theme mode.
    pub mode: ThemeMode,
    /// The typography density.
    pub density: Density,
    /// The tree, in pre-order.
    pub nodes: Vec<NodeDebug>,
    /// The number of display commands.
    pub command_count: usize,
    /// This frame's damage.
    pub damage: Vec<Rect>,
    /// Layout counters.
    pub stats: LayoutStats,
    /// The number of messages emitted this frame.
    pub messages: usize,
}

impl FrameDebug {
    /// A node by its stable path.
    #[must_use]
    pub fn node(&self, path: &str) -> Option<&NodeDebug> {
        self.nodes.iter().find(|node| node.path == path)
    }

    /// The union of this frame's damage.
    #[must_use]
    pub fn damage_union(&self) -> Option<Rect> {
        crate::layout::damage_union(&self.damage)
    }
}

#[cfg(test)]
mod tests {
    use super::CacheState;
    use crate::geom::Rect;
    use crate::layout::LayoutStats;
    use crate::store::RepaintReason;
    use crate::theme::{Density, ThemeMode};

    #[test]
    fn cache_state_is_plain_data() {
        let state = CacheState {
            layout_reused: true,
            text_reused: true,
            paint_reused: false,
        };
        assert!(state.layout_reused);
        assert!(!state.paint_reused);
        assert_eq!(RepaintReason::HoverChanged.name(), "hover_changed");
        let layout_stats = LayoutStats::default();
        assert_eq!(layout_stats.nodes, 0);
        assert_eq!(ThemeMode::Dark, ThemeMode::Dark);
        assert_eq!(Density::Compact, Density::Compact);
        let _ = Rect::ZERO;
    }
}
