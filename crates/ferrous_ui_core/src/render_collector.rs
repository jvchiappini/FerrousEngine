use crate::context::DrawContext;
use crate::primitives::{Overflow, Rect};
use crate::render_command::CapturedCommand;
use crate::ui_tree::{NodeId, UiTree};
use crate::RenderCommand;

/// The cached commands of a single node, handed to the batch builder.
///
/// The collector emits one of these per visited node, in paint order, with the
/// node's own commands only (children come as their own visits). The batch
/// builder can reuse the batch products it made for this node on the previous
/// frame when [`Self::redrawn`] is `false` and the rest of the key matches.
pub struct NodeBatch<'a> {
    /// Full slot-map key (index and generation), stable per logical node.
    ///
    /// Unlike [`Self::node_id`], this never repeats when a removed slot is
    /// reused, so it is safe as a retained-cache key.
    pub handle: u64,
    /// Low 32-bit handle used as the GPU hit-test id (`node_id` in the shader).
    pub node_id: u32,
    /// Depth of this node in the paint order.
    pub z: f32,
    /// True when the widget's `draw` ran this frame, i.e. the commands may
    /// have changed. `false` means the cached commands are byte-identical to
    /// the ones the previous frame saw.
    pub redrawn: bool,
    /// Rect of the node's overflow clip, when it clips its content and children.
    pub overflow: Option<Rect>,
    /// The node's own cached commands.
    pub cmds: &'a [RenderCommand],
}

/// One event of a collection walk.
pub enum NodeVisit<'a> {
    /// Emit one node's own commands (and open its overflow clip, if any).
    Body(NodeBatch<'a>),
    /// Close the overflow clip opened by the most recent [`Self::Body`].
    ///
    /// Emitted after the node's children so the clip wraps the whole subtree,
    /// exactly like the previous single-pass collector did. Carries the owning
    /// node's depth and GPU id so the owned [`RenderCollector::collect`] form
    /// can reproduce the old captured-command metadata.
    PopClip {
        /// Depth of the node that owns the clip.
        z: f32,
        /// Low 32-bit handle of the node that owns the clip.
        node_id: u32,
    },
}

pub struct RenderCollector;

impl RenderCollector {
    /// Collects render commands from the UI tree, starting from the root.
    ///
    /// The owned form is kept for callers that need a list; the renderer uses
    /// [`Self::collect_with`] so a clean frame does not clone every command or
    /// every child list.
    pub fn collect<App>(tree: &mut UiTree<App>, cmds: &mut Vec<CapturedCommand>, viewport: Rect) {
        cmds.clear();
        Self::collect_with(tree, viewport, |visit| match visit {
            NodeVisit::Body(node) => {
                if let Some(rect) = node.overflow {
                    cmds.push(CapturedCommand {
                        cmd: RenderCommand::PushClip { rect },
                        z: node.z,
                        node_id: node.node_id,
                    });
                }
                for cmd in node.cmds {
                    cmds.push(CapturedCommand {
                        cmd: cmd.clone(),
                        z: node.z,
                        node_id: node.node_id,
                    });
                }
            }
            NodeVisit::PopClip { z, node_id } => cmds.push(CapturedCommand {
                cmd: RenderCommand::PopClip,
                z,
                node_id,
            }),
        });
    }

    /// Walks the tree and emits every node by reference.
    ///
    /// Nothing is cloned on the steady-state (clean-subtree) path: the cached
    /// commands are borrowed and the child list is walked by index, so a frame
    /// over an unchanged tree allocates no heap memory.
    ///
    /// Subtrees whose rect does not intersect the accumulated ancestor clip
    /// (the viewport reduced by the overflow clips on the path from the root)
    /// are skipped entirely, so fully clipped scroll-view rows are neither
    /// collected nor batched.
    pub fn collect_with<App, F>(tree: &mut UiTree<App>, viewport: Rect, mut emit: F)
    where
        F: for<'a> FnMut(NodeVisit<'a>),
    {
        if let Some(root_id) = tree.get_root() {
            Self::collect_node_with(tree, root_id, viewport, viewport, 0.0, &mut emit);
        }
    }

    /// Visibility of a node against the accumulated ancestor clip.
    ///
    /// Lane A hook. `UiTree::visible_rect(&self, id: NodeId) -> Option<Rect>`
    /// exists and this walk already reproduces it in O(1) per node by
    /// intersecting each clipping ancestor's rect once (node rects are absolute
    /// window coordinates). If `UiTree::clip_rect(&self, id: NodeId) -> Rect`
    /// (empty rect = fully clipped) also lands, `is_visible` can be written as
    /// `tree.visible_rect(id).is_some_and(|r| r.intersects(&viewport))
    ///  && { let clip = tree.clip_rect(id); clip.width > 0.0 && clip.height > 0.0 }`
    /// and the `clip` parameter below can be dropped.
    #[inline]
    fn is_visible(rect: Rect, clip: Rect) -> bool {
        rect.intersects(&clip)
    }

    /// Clip inherited by the children of a node with the given overflow state.
    #[inline]
    fn child_clip(clip: Rect, node_rect: Rect, clips_overflow: bool) -> Rect {
        if clips_overflow {
            clip.intersect(&node_rect)
        } else {
            clip
        }
    }

    fn collect_node_with<App, F>(
        tree: &mut UiTree<App>,
        id: NodeId,
        viewport: Rect,
        clip: Rect,
        z: f32,
        emit: &mut F,
    ) where
        F: for<'a> FnMut(NodeVisit<'a>),
    {
        let Some(rect) = tree.get_node(id).map(|node| node.rect) else {
            return;
        };
        if !Self::is_visible(rect, clip) {
            return;
        }
        let handle = slotmap::Key::data(&id).as_ffi();
        let node_id_val = handle as u32;

        // FAST PATH: clean subtree and cache available.
        let clean = tree
            .get_node(id)
            .is_some_and(|node| !node.dirty.subtree_dirty && !node.cached_cmds.is_empty());
        let mut redrawn = false;
        if !clean {
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
                redrawn = true;
            }
        }

        let clips_overflow = tree
            .get_node(id)
            .is_some_and(|node| node.style.overflow != Overflow::Visible);
        let overflow = clips_overflow.then_some(rect);
        if let Some(node) = tree.get_node(id) {
            emit(NodeVisit::Body(NodeBatch {
                handle,
                node_id: node_id_val,
                z,
                redrawn,
                overflow,
                cmds: &node.cached_cmds,
            }));
        }
        let child_clip = Self::child_clip(clip, rect, clips_overflow);
        Self::walk_children(tree, id, viewport, child_clip, z, emit);
        if overflow.is_some() {
            emit(NodeVisit::PopClip {
                z,
                node_id: node_id_val,
            });
        }
        if !clean {
            if let Some(node) = tree.get_node_mut(id) {
                node.dirty.subtree_dirty = false;
            }
        }
    }

    /// Recurses into the children without cloning the child list.
    fn walk_children<App, F>(
        tree: &mut UiTree<App>,
        id: NodeId,
        viewport: Rect,
        clip: Rect,
        z: f32,
        emit: &mut F,
    ) where
        F: for<'a> FnMut(NodeVisit<'a>),
    {
        let count = tree.get_node(id).map_or(0, |node| node.children.len());
        for index in 0..count {
            let child = tree
                .get_node(id)
                .and_then(|node| node.children.get(index).copied());
            if let Some(child) = child {
                Self::collect_node_with(tree, child, viewport, clip, z + 0.001, emit);
            }
        }
    }
}
