use crate::ui_tree::{NodeId, UiTree};
use crate::primitives::{Rect, Overflow};
use crate::RenderCommand;
use crate::render_command::CapturedCommand;
use crate::context::DrawContext;

pub struct RenderCollector;

impl RenderCollector {
    /// Collects render commands from the UI tree, starting from the root.
    pub fn collect<App>(tree: &mut UiTree<App>, cmds: &mut Vec<CapturedCommand>, viewport: Rect) {
        if let Some(root_id) = tree.get_root() {
            static mut VISITED: u32 = 0;
            static mut TOTAL_FRAMES: u32 = 0;
            
            unsafe {
                VISITED = 0;
                Self::collect_node(tree, root_id, cmds, viewport, 0.0, &mut VISITED);
                TOTAL_FRAMES += 1;
                if TOTAL_FRAMES.is_multiple_of(120) {
                    println!("[RenderCollector] Visited {} nodes, collected {} commands", VISITED, cmds.len());
                }
            }
        }
    }

    fn collect_node<App>(tree: &mut UiTree<App>, id: NodeId, cmds: &mut Vec<CapturedCommand>, viewport: Rect, z: f32, count: &mut u32) {
        *count += 1;
        let node = match tree.get_node(id) {
            Some(n) => n,
            None => return,
        };
        
        // Convert NodeId to a numeric value for the GPU hit-testing buffer
        let node_id_val = slotmap::Key::data(&id).as_ffi() as u32;

        if !node.rect.intersects(&viewport) {
            return;
        }

        // FAST PATH: clean subtree and cache available
        if !node.dirty.subtree_dirty && !node.cached_cmds.is_empty() {
            let overflow_clip = node.style.overflow != Overflow::Visible;
            if overflow_clip {
                cmds.push(CapturedCommand { cmd: RenderCommand::PushClip { rect: node.rect }, z, node_id: node_id_val });
            }
            for cmd in &node.cached_cmds {
                cmds.push(CapturedCommand { cmd: cmd.clone(), z, node_id: node_id_val });
            }
            
            let children = node.children.clone();
            // Painter's Algorithm: process in normal order (background to foreground)
            for child_id in children {
                Self::collect_node(tree, child_id, cmds, viewport, z + 0.001, count);
            }
            
            if overflow_clip {
                cmds.push(CapturedCommand { cmd: RenderCommand::PopClip, z, node_id: node_id_val });
            }
            return;
        }

        // DIRTY PATH: regenerate commands
        let is_dirty = tree.get_node(id).map(|n| n.dirty.is_dirty()).unwrap_or(false);
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

        let (overflow_clip, children) = if let Some(node) = tree.get_node(id) {
            let clip = node.style.overflow != Overflow::Visible;
            if clip {
                cmds.push(CapturedCommand { cmd: RenderCommand::PushClip { rect: node.rect }, z, node_id: node_id_val });
            }
            for cmd in &node.cached_cmds {
                cmds.push(CapturedCommand { cmd: cmd.clone(), z, node_id: node_id_val });
            }
            (clip, node.children.clone())
        } else {
            return;
        };

        // Draw children in normal order (background to foreground)
        for child_id in children {
            Self::collect_node(tree, child_id, cmds, viewport, z + 0.001, count);
        }

        if overflow_clip {
            cmds.push(CapturedCommand { cmd: RenderCommand::PopClip, z, node_id: node_id_val });
        }

        if let Some(node) = tree.get_node_mut(id) {
            node.dirty.subtree_dirty = false;
        }
    }
}
