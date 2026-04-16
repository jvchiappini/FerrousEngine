//! UI Tree System for Ferrous Engine
//!
//! This module contains the core tree structure for the retained-mode UI system.
//! It includes NodeId, DirtyFlags, CmdQueue, Node, and UiTree with their implementations.

use slotmap::{new_key_type, SlotMap};

use crate::primitives::{Rect, Style};
use crate::reactive::ReactivitySystem;
use crate::theme::Theme;
use crate::context::{BuildContext, UpdateContext};
use crate::Widget;
use crate::RenderCommand;
use glam::Vec2;

// ── Type Definitions ──────────────────────────────────────────────────────────

new_key_type! {
    /// Stable and unique identifier for a node within the `UiTree`.
    pub struct NodeId;
}

/// Flags indicating which aspects of a node or its subtree need updating.
/// This system is key to achieving "Zero Lag" by skipping clean branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirtyFlags {
    /// Indicates that the node's size or position must be recalculated.
    pub layout: bool,
    /// Indicates that the node's visuals have changed and it must regenerate its `RenderCommand`s.
    pub paint: bool,
    /// Indicates that the hierarchy (children) has changed.
    pub hierarchy: bool,
    /// Propagation: true if this node or any of its descendants are dirty.
    /// Allows skipping entire branches during traversal if false.
    pub subtree_dirty: bool,
}

/// Deferred command queue for the UI.
/// Allows widgets to request actions that must happen outside the event cycle
/// (e.g., opening a window, closing the app).
pub struct CmdQueue {
    // TODO: Implement deferred command variants
}

// ── DirtyFlags Implementation ──────────────────────────────────────────────────

impl DirtyFlags {
    /// Creates a set of "clean" flags.
    pub fn none() -> Self {
        Self::default()
    }

    /// Creates a set of flags where everything is marked as dirty.
    pub fn all() -> Self {
        Self {
            layout: true,
            paint: true,
            hierarchy: true,
            subtree_dirty: true,
        }
    }

    /// Checks if the local node has any pending updates.
    pub fn is_dirty(&self) -> bool {
        self.layout || self.paint || self.hierarchy
    }
}

// ── CmdQueue Implementation ────────────────────────────────────────────────────

impl Default for CmdQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl CmdQueue {
    pub fn new() -> Self {
        Self {}
    }
}

// ── Node Definition ────────────────────────────────────────────────────────────

/// Contains a widget and all metadata necessary for its management and optimized rendering.
pub struct Node<App> {
    pub widget: Box<dyn Widget<App>>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub style: Style,
    pub dirty: DirtyFlags,
    /// Final rectangle resolved by the layout engine in local coordinates.
    pub rect: Rect,
    /// Cache of drawing commands generated in the last frame where the node was "dirty".
    pub cached_cmds: Vec<RenderCommand>,
    /// Opaque ID of the corresponding node in the Taffy tree.
    /// Stored here to avoid a per-frame HashMap lookup in ferrous_layout.
    pub taffy_id: Option<u64>,
}

// ── UiTree Definition ──────────────────────────────────────────────────────────

/// Main manager for the widget tree.
/// Maintains the hierarchy using a `SlotMap` to guarantee O(1) access and ID stability.
pub struct UiTree<App> {
    nodes: SlotMap<NodeId, Node<App>>,
    root: Option<NodeId>,
    /// Mapping of text identifiers to NodeIds for fast lookups.
    id_map: std::collections::HashMap<String, NodeId>,
    /// System managing reactive updates for nodes.
    pub reactivity: ReactivitySystem,
    pub theme: Theme,
    /// Spatial index for fast hit-testing.
    pub spatial_index: crate::spatial_index::SpatialIndex,
    /// Regions that changed visually in the last frame.
    pub damage_regions: Vec<Rect>,
}

// ── UiTree Implementation ──────────────────────────────────────────────────────

impl<App> UiTree<App> {
    /// Creates an empty UI tree.
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            root: None,
            id_map: std::collections::HashMap::new(),
            reactivity: ReactivitySystem::new(),
            theme: Theme::default(),
            spatial_index: crate::spatial_index::SpatialIndex::new(),
            damage_regions: Vec::new(),
        }
    }

    pub fn get_root(&self) -> Option<NodeId> {
        self.root
    }

    /// Gets a mutable reference to a node in the tree.
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut Node<App>> {
        self.nodes.get_mut(id)
    }

    /// Gets an immutable reference to a node in the tree.
    pub fn get_node(&self, id: NodeId) -> Option<&Node<App>> {
        self.nodes.get(id)
    }

    /// Executes the recursive build phase from the root.
    pub fn build(&mut self) {
        if let Some(root_id) = self.root {
            self.build_node(root_id);
        }
    }

    fn build_node(&mut self, id: NodeId) {
        // Clear children before rebuild to handle dynamic children correctly
        if let Some(node) = self.nodes.get_mut(id) {
            node.children.clear();
        }

        // Temporarily extract the widget to avoid double borrowing the tree
        // while calling widget.build(&mut ctx).
        let mut widget = if let Some(node) = self.nodes.get_mut(id) {
            std::mem::replace(
                &mut node.widget,
                Box::new(crate::widgets::PlaceholderWidget),
            )
        } else {
            return;
        };

        let theme = self.theme;
        let mut ctx = BuildContext {
            tree: self,
            node_id: id,
            theme,
        };
        widget.build(&mut ctx);

        let children = if let Some(node) = self.nodes.get_mut(id) {
            node.widget = widget;
            node.children.clone()
        } else {
            return;
        };

        for child_id in children {
            self.build_node(child_id);
        }
    }

    /// Updates the logic of all widgets in the tree.
    pub fn update(&mut self, delta_time: f32) {
        // Collect dirty nodes from reactivity system before mutation
        let dirty_nodes = std::mem::take(&mut self.reactivity.pending_dirty_nodes);
        for id in dirty_nodes {
            self.mark_paint_dirty(id);
        }

        if let Some(root_id) = self.root {
            self.update_node(root_id, delta_time);
        }
    }

    pub fn update_spatial_index(&mut self) {
        let mut index = std::mem::take(&mut self.spatial_index);
        index.update_if_dirty(self);
        self.spatial_index = index;
    }

    pub fn clear_damage(&mut self) {
        self.damage_regions.clear();
    }

    /// Marks a node as dirty for layout and propagates the flag to parents.
    pub fn mark_layout_dirty(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.dirty.subtree_dirty = true;
            if !node.dirty.layout {
                self.damage_regions.push(node.rect);
                node.dirty.layout = true;
                if let Some(parent_id) = node.parent {
                    self.mark_layout_dirty(parent_id);
                }
            }
        }
    }

    /// Marks a node as dirty for repainting.
    pub fn mark_paint_dirty(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.dirty.subtree_dirty = true;
            if !node.dirty.paint {
                self.damage_regions.push(node.rect);
                node.dirty.paint = true;
                if let Some(parent_id) = node.parent {
                    self.mark_subtree_dirty_up(parent_id);
                }
            }
        }
    }

    fn update_node(&mut self, id: NodeId, delta_time: f32) {
        let (children, node_rect) = if let Some(node) = self.nodes.get(id) {
            (node.children.clone(), node.rect)
        } else {
            return;
        };

        let mut content_max = Vec2::ZERO;

        for child_id in children {
            self.update_node(child_id, delta_time);
            if let Some(child_node) = self.nodes.get(child_id) {
                let r = child_node.rect;
                content_max.x = content_max.x.max((r.x - node_rect.x).max(0.0) + r.width);
                content_max.y = content_max.y.max((r.y - node_rect.y).max(0.0) + r.height);
            }
        }

        if let Some(node) = self.nodes.get_mut(id) {
            let theme = self.theme;
            let mut ctx = UpdateContext {
                delta_time,
                node_id: id,
                rect: node.rect,
                content_size: content_max,
                theme,
                needs_redraw: false,
            };
            node.widget.update(&mut ctx);
            if ctx.needs_redraw {
                if !node.dirty.paint {
                    self.damage_regions.push(node.rect);
                }
                node.dirty.paint = true;
                node.dirty.subtree_dirty = true;
            }
        }
    }

    fn mark_subtree_dirty_up(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            if !node.dirty.subtree_dirty {
                node.dirty.subtree_dirty = true;
                if let Some(parent_id) = node.parent {
                    self.mark_subtree_dirty_up(parent_id);
                }
            }
        }
    }

    /// Inserts a new node into the tree.
    pub fn add_node(&mut self, widget: Box<dyn Widget<App>>, parent: Option<NodeId>) -> NodeId {
        self.add_node_with_id(widget, parent, None)
    }

    /// Inserts a new node into the tree with an optional identifier.
    pub fn add_node_with_id(
        &mut self,
        widget: Box<dyn Widget<App>>,
        parent: Option<NodeId>,
        id_str: Option<String>,
    ) -> NodeId {
        let id = self.nodes.insert(Node {
            widget,
            parent,
            children: Vec::new(),
            style: Style::default(),
            dirty: DirtyFlags::all(),
            rect: Rect::default(),
            cached_cmds: Vec::new(),
            taffy_id: None,
        });

        if let Some(s) = id_str {
            self.id_map.insert(s, id);
        }

        if let Some(parent_id) = parent {
            if let Some(parent_node) = self.nodes.get_mut(parent_id) {
                parent_node.children.push(id);
                parent_node.dirty.hierarchy = true;
                self.mark_layout_dirty(parent_id);
            }
        } else if self.root.is_none() {
            self.root = Some(id);
        }

        id
    }

    /// Removes a node and all its children from the tree.
    pub fn remove_node(&mut self, id: NodeId) {
        let (parent, rect, children) = match self.nodes.get(id) {
            Some(node) => (node.parent, node.rect, node.children.clone()),
            None => return,
        };

        // 1. Mark parent as dirty
        if let Some(parent_id) = parent {
            if let Some(parent_node) = self.nodes.get_mut(parent_id) {
                parent_node.children.retain(|&child| child != id);
                parent_node.dirty.hierarchy = true;
            }
            self.mark_layout_dirty(parent_id);
        } else if self.root == Some(id) {
            self.root = None;
        }

        // 2. Add node rect to damage regions
        self.damage_regions.push(rect);

        // 3. Remove recursively
        for child_id in children {
            self.remove_node_recursive(child_id);
        }

        // 4. Remove from id_map
        self.id_map.retain(|_, &mut v| v != id);

        // 5. Finally remove from slotmap
        self.nodes.remove(id);
    }

    fn remove_node_recursive(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.remove(id) {
            for child_id in node.children {
                self.remove_node_recursive(child_id);
            }
        }
    }

    /// Replaces a widget in an existing node, preserving hierarchy.
    pub fn replace_node(&mut self, id: NodeId, new_widget: Box<dyn Widget<App>>) {
        let exists = if let Some(node) = self.nodes.get_mut(id) {
            node.widget = new_widget;
            node.dirty.paint = true;
            node.dirty.layout = true;
            true
        } else {
            false
        };

        if exists {
            self.mark_layout_dirty(id);
        }
    }

    /// Gets children of a node.
    pub fn get_node_children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(id).map(|n| n.children.as_slice())
    }

    /// Gets the style of a node.
    pub fn get_node_style(&self, id: NodeId) -> Option<&Style> {
        self.nodes.get(id).map(|n| &n.style)
    }

    /// Sets the style of a node and marks it as dirty for layout.
    pub fn set_node_style(&mut self, id: NodeId, style: Style) {
        let exists = if let Some(node) = self.nodes.get_mut(id) {
            node.style = style;
            true
        } else {
            false
        };

        if exists {
            self.mark_layout_dirty(id);
        }
    }

    /// Gets the resolved rectangle of a node.
    pub fn get_node_rect(&self, id: NodeId) -> Option<Rect> {
        self.nodes.get(id).map(|n| n.rect)
    }

    /// Gets the parent of a node.
    pub fn get_node_parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(id).and_then(|n| n.parent)
    }

    /// Sets the rectangle of a node and marks it as dirty for repainting.
    pub fn set_node_rect(&mut self, id: NodeId, rect: Rect) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.rect = rect;
            node.dirty.paint = true;
            node.dirty.subtree_dirty = true;
        }
    }

    /// Finds a node by its text identifier.
    pub fn get_node_by_id(&self, id_str: &str) -> Option<NodeId> {
        self.id_map.get(id_str).copied()
    }
}