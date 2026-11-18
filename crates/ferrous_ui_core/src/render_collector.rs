use crate::context::DrawContext;
use crate::primitives::{Overflow, Rect};
use crate::render_command::CapturedCommand;
use crate::ui_tree::{NodeId, UiTree};
use crate::RenderCommand;

pub struct RenderCollector;

impl RenderCollector {
    /// Collects render commands from the UI tree, starting from the root.
    ///
    /// The owned form is kept for callers that need a list; the renderer uses
    /// [`Self::collect_with`] so a clean frame does not clone every command or
    /// every child list.
    pub fn collect<App>(tree: &mut UiTree<App>, cmds: &mut Vec<CapturedCommand>, viewport: Rect) {
        cmds.clear();
        Self::collect_with(tree, viewport, |cmd, z, node_id| {
            cmds.push(CapturedCommand {
                cmd: cmd.clone(),
                z,
                node_id,
            });
        });
    }

    /// Walks the tree and emits every command by reference.
    ///
    /// Nothing is cloned on the steady-state (clean-subtree) path: the cached
    /// commands are borrowed and the child list is walked by index, so a frame
    /// over an unchanged tree allocates no heap memory.
    pub fn collect_with<App, F>(tree: &mut UiTree<App>, viewport: Rect, mut emit: F)
    where
        F: FnMut(&RenderCommand, f32, u32),
    {
        if let Some(root_id) = tree.get_root() {
            Self::collect_node_with(tree, root_id, viewport, 0.0, &mut emit);
        }
    }

    fn collect_node_with<App, F>(
        tree: &mut UiTree<App>,
        id: NodeId,
        viewport: Rect,
        z: f32,
        emit: &mut F,
    ) where
        F: FnMut(&RenderCommand, f32, u32),
    {
        let Some(rect) = tree.get_node(id).map(|node| node.rect) else {
            return;
        };
        if !rect.intersects(&viewport) {
            return;
        }
        let node_id_val = slotmap::Key::data(&id).as_ffi() as u32;

        // FAST PATH: clean subtree and cache available.
        let clean = tree
            .get_node(id)
            .is_some_and(|node| !node.dirty.subtree_dirty && !node.cached_cmds.is_empty());
        if clean {
            let overflow_clip = tree
                .get_node(id)
                .is_some_and(|node| node.style.overflow != Overflow::Visible);
            if overflow_clip {
                let cmd = RenderCommand::PushClip { rect };
                emit(&cmd, z, node_id_val);
            }
            if let Some(node) = tree.get_node(id) {
                for cmd in &node.cached_cmds {
                    emit(cmd, z, node_id_val);
                }
            }
            Self::walk_children(tree, id, viewport, z, emit);
            if overflow_clip {
                let cmd = RenderCommand::PopClip;
                emit(&cmd, z, node_id_val);
            }
            return;
        }

        // DIRTY PATH: regenerate this node's commands once.
        let is_dirty = tree.get_node(id).is_some_and(|node| node.dirty.is_dirty());
        if is_dirty {
            let theme = tree.theme;
            if let Some(node) = tree.get_node_mut(id) {
                node.cached_cmds.clear();
                let mut ctx = DrawContext {
                    node_id: id,
                    rect: node.rect,
                    theme,
                };
                node.widget.draw(&mut ctx, &mut node.cached_cmds);
                node.dirty.paint = false;
                node.dirty.layout = false;
                node.dirty.hierarchy = false;
            }
        }

        let overflow_clip = tree
            .get_node(id)
            .is_some_and(|node| node.style.overflow != Overflow::Visible);
        if overflow_clip {
            let cmd = RenderCommand::PushClip { rect };
            emit(&cmd, z, node_id_val);
        }
        if let Some(node) = tree.get_node(id) {
            for cmd in &node.cached_cmds {
                emit(cmd, z, node_id_val);
            }
        }
        Self::walk_children(tree, id, viewport, z, emit);
        if overflow_clip {
            let cmd = RenderCommand::PopClip;
            emit(&cmd, z, node_id_val);
        }
        if let Some(node) = tree.get_node_mut(id) {
            node.dirty.subtree_dirty = false;
        }
    }

    /// Recurses into the children without cloning the child list.
    fn walk_children<App, F>(
        tree: &mut UiTree<App>,
        id: NodeId,
        viewport: Rect,
        z: f32,
        emit: &mut F,
    ) where
        F: FnMut(&RenderCommand, f32, u32),
    {
        let count = tree.get_node(id).map_or(0, |node| node.children.len());
        for index in 0..count {
            let child = tree
                .get_node(id)
                .and_then(|node| node.children.get(index).copied());
            if let Some(child) = child {
                Self::collect_node_with(tree, child, viewport, z + 0.001, emit);
            }
        }
    }
}
