use ferrous_events::EventManager;
use ferrous_layout::LayoutEngine;
use ferrous_ui_core::{NodeId, Rect, UiEvent, UiTree, Widget};
use ferrous_ui_core::render_collector::RenderCollector;
use ferrous_ui_render::{GuiBatch, ToBatches};
use crate::toaster::Toaster;


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
    pub fn update(&mut self, dt: f32, viewport_width: f32, viewport_height: f32) 
    {
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
        let mut captured = Vec::new();
        RenderCollector::collect(&mut self.tree, &mut captured, viewport);

        let mut batch = GuiBatch::new();
        for cap in captured {
            cap.cmd.to_batches(&mut batch, font, cap.z, cap.node_id);
        }
        
        // Calculate and attach the damage union for the current frame.
        batch.damage_union = self.damage_union();
        batch
    }

    /// Render version when text support is disabled.
    #[cfg(not(feature = "text"))]
    pub fn render(&mut self, viewport: Rect) -> GuiBatch {
        let mut captured = Vec::new();
        RenderCollector::collect(&mut self.tree, &mut captured, viewport);

        let mut batch = GuiBatch::new();
        for cap in captured {
            cap.cmd.to_batches(&mut batch, cap.z, cap.node_id);
        }
        batch.damage_union = self.damage_union();
        batch
    }

    /// Returns the union of all damaged regions in this frame.
    pub fn damage_union(&self) -> Option<Rect> {
        let regions: Vec<_> = self.tree.damage_regions.iter()
            .filter(|r| r.width > 0.0 && r.height > 0.0)
            .collect();
            
        if regions.is_empty() {
            return None;
        }
        
        let mut union = *regions[0];
        for rect in &regions[1..] {
            union = union.union(**rect);
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
