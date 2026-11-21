use crate::toaster::Toaster;
use ferrous_events::EventManager;
use ferrous_layout::LayoutEngine;
use ferrous_ui_core::render_collector::{NodeVisit, RenderCollector};
use ferrous_ui_core::{NodeId, Rect, UiEvent, UiTree, Widget};
use ferrous_ui_render::GuiBatch;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide identity for each [`UiSystem`].
///
/// A recycled [`GuiBatch`] may outlive the tree it was filled from; the epoch
/// lets the batch drop retained node ranges instead of matching a new tree's
/// reused slot-map handles.
static NEXT_RENDER_EPOCH: AtomicU64 = AtomicU64::new(1);

/// `UiSystem` is the master orchestrator of the UI system.
///
/// Coordinates widget state (`UiTree`), the layout engine (`LayoutEngine`),
/// event management (`EventManager`), and facilitates draw command generation.
pub struct UiSystem<App> {
    /// Widget tree containing hierarchy and reactive state.
    pub tree: UiTree<App>,
    /// Engine responsible for calculating positions and sizes using Taffy (Flexbox).
    pub layout_engine: LayoutEngine,
    /// Manager for interactions, focus, and hover.
    pub event_manager: EventManager,
    /// Stack of implicit parents for `spawn_with`.
    /// When not empty, `.spawn()` inserts the widget as a child of the top node.
    pub(crate) parent_stack: Vec<NodeId>,
    /// Global toaster manager.
    pub toaster: Toaster<App>,
    /// Unique identity used by the retained batch store in [`GuiBatch`].
    render_epoch: u64,
}

impl<App: 'static> UiSystem<App> {
    /// Creates a new UI system with default settings.
    pub fn new() -> Self {
        Self {
            tree: UiTree::new(),
            layout_engine: LayoutEngine::new(),
            event_manager: EventManager::new(),
            parent_stack: Vec::new(),
            toaster: Toaster::new(),
            render_epoch: NEXT_RENDER_EPOCH.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Pushes an implicit parent. Used by `PanelBuilder::spawn_with`.
    pub fn push_parent(&mut self, id: NodeId) {
        self.parent_stack.push(id);
    }

    /// Pops the last implicit parent.
    pub fn pop_parent(&mut self) {
        self.parent_stack.pop();
    }

    /// Returns the current implicit parent, if one exists.
    pub fn current_parent(&self) -> Option<NodeId> {
        self.parent_stack.last().copied()
    }

    /// Adds a widget to the system root (alias of add_node).
    pub fn add(&mut self, widget: impl Widget<App> + 'static) -> NodeId {
        self.tree.add_node(Box::new(widget), None)
    }

    /// Registers a special Viewport widget.
    /// In this version, it simply adds it as a regular node.
    pub fn register_viewport(&mut self, viewport: impl Widget<App> + 'static) -> NodeId {
        self.tree.add_node(Box::new(viewport), None)
    }

    /// Entry point for the update cycle.
    /// Processes reactivity and recalculates element layout.
    pub fn update(&mut self, dt: f32, viewport_width: f32, viewport_height: f32) {
        // 0. Update toaster system
        Toaster::update_system(self, dt);

        // 1. Update internal widget logic and reactive system.
        self.tree.update(dt);

        // 2. Recalculate layout for the entire tree.
        // Taffy is efficient and only recalculates changed nodes (Dirty Nodes).
        self.layout_engine
            .compute_layout(&mut self.tree, viewport_width, viewport_height);

        // 3. Update UI AABB index to accelerate hit-testing collisions to O(1).
        self.tree.update_spatial_index();
    }

    /// Dispatches an input event to the system.
    /// Automatically handles Hit-Testing and event propagation (Bubbling).
    pub fn dispatch_event(&mut self, app: &mut App, event: UiEvent) {
        self.event_manager
            .dispatch_event(&mut self.tree, app, event);
    }

    /// Generates a batch of quads ready to be sent to the rendering backend (WGPU).
    /// Performs automatic culling based on the provided viewport.
    #[cfg(feature = "text")]
    pub fn render(&mut self, viewport: Rect, font: Option<&ferrous_assets::Font>) -> GuiBatch {
        let mut batch = GuiBatch::new();
        self.render_into(&mut batch, viewport, font);
        batch
    }

    /// Appends this frame's commands to a reused batch.
    ///
    /// The runner passes the batch it reclaimed from the renderer, so a steady
    /// frame reuses every Vec's capacity. Commands are emitted by reference:
    /// no captured-command list and no child-list clones. On top of that, the
    /// batch retains each node's previous batch ranges, so a clean node is
    /// replayed with `extend_from_slice` instead of `to_batches`; a hover that
    /// dirties one node only converts that node.
    #[cfg(feature = "text")]
    pub fn render_into(
        &mut self,
        batch: &mut GuiBatch,
        viewport: Rect,
        font: Option<&ferrous_assets::Font>,
    ) {
        batch.begin_retained_frame(self.render_epoch);
        RenderCollector::collect_with(&mut self.tree, viewport, |visit| match visit {
            NodeVisit::Body(node) => batch.push_node(node, font),
            NodeVisit::PopClip { .. } => batch.pop_clip(),
        });
        batch.end_retained_frame();
        batch.damage_union = self.damage_union();
    }

    /// Render version when text support is disabled.
    #[cfg(not(feature = "text"))]
    pub fn render(&mut self, viewport: Rect) -> GuiBatch {
        let mut batch = GuiBatch::new();
        self.render_into(&mut batch, viewport);
        batch
    }

    /// Appends this frame's commands to a reused batch (no-text build).
    #[cfg(not(feature = "text"))]
    pub fn render_into(&mut self, batch: &mut GuiBatch, viewport: Rect) {
        batch.begin_retained_frame(self.render_epoch);
        RenderCollector::collect_with(&mut self.tree, viewport, |visit| match visit {
            NodeVisit::Body(node) => batch.push_node(node),
            NodeVisit::PopClip { .. } => batch.pop_clip(),
        });
        batch.end_retained_frame();
        batch.damage_union = self.damage_union();
    }

    /// Returns the union of all damaged regions in this frame.
    pub fn damage_union(&self) -> Option<Rect> {
        let mut regions = self
            .tree
            .damage_regions
            .iter()
            .filter(|rect| rect.width > 0.0 && rect.height > 0.0);
        let mut union = *regions.next()?;
        for rect in regions {
            union = union.union(*rect);
        }
        Some(union)
    }

    /// Clears the list of damaged regions after rendering.
    pub fn clear_damage(&mut self) {
        self.tree.clear_damage();
    }

    // =========================================================================
    // Fluent API — convenience for creating widgets without touching UiTree directly
    // =========================================================================

    /// Starts building a button with text.
    pub fn button(&mut self, label: impl Into<String>) -> crate::builder::ButtonBuilder<App> {
        crate::builder::ButtonBuilder::new(label)
    }

    /// Starts building a label (static or dynamic text).
    pub fn label(&mut self, text: impl Into<String>) -> crate::builder::LabelBuilder<App> {
        crate::builder::LabelBuilder::new(text)
    }

    /// Starts building an icon (MSDF vector icon).
    pub fn icon(&mut self, name: impl Into<String>) -> crate::builder::IconBuilder<App> {
        crate::builder::IconBuilder::new(name)
    }

    /// Shows a toast notification.
    pub fn show_toast(&mut self, message: impl Into<String>, duration: f32) {
        Toaster::show_system(self, message, duration);
    }

    /// Starts building a panel (container with background).
    pub fn panel(&mut self) -> crate::builder::PanelBuilder<App> {
        crate::builder::PanelBuilder::new()
    }

    /// Starts building any custom widget.
    pub fn widget(
        &mut self,
        widget: impl Widget<App> + 'static,
    ) -> crate::builder::WidgetBuilder<App> {
        crate::builder::WidgetBuilder::new(widget)
    }
}

impl<App: 'static> Default for UiSystem<App> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "text"))]
mod retained_tests {
    use super::*;
    use ferrous_ui_core::{DrawContext, RenderCommand};

    struct Solid;

    impl Widget<()> for Solid {
        fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
            cmds.push(RenderCommand::Quad {
                rect: ctx.rect,
                color: [1.0; 4],
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    #[test]
    fn a_single_node_repaint_reports_only_its_rect_as_damage() {
        let mut ui = UiSystem::<()>::new();
        let root = ui.add(Solid);
        let child = ui.tree.add_node(Box::new(Solid), Some(root));
        let root_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        };
        let child_rect = Rect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        };
        ui.tree.set_node_rect(root, root_rect);
        ui.tree.set_node_rect(child, child_rect);
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };

        let mut batch = GuiBatch::new();
        ui.render_into(&mut batch, viewport, None);
        ui.clear_damage();
        ui.tree.mark_paint_dirty(child);
        batch.clear();
        ui.render_into(&mut batch, viewport, None);

        assert_eq!(
            batch.damage_union,
            Some(child_rect),
            "a one-node repaint must damage only that node's rect"
        );
    }
}
