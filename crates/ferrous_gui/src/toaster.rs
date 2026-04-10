use std::collections::VecDeque;
use ferrous_ui_core::{NodeId, Color};
use crate::system::UiSystem;

/// Represents a single toast notification.
pub struct Toast {
    pub message: String,
    pub duration: f32,
    pub timer: f32,
    pub node_id: Option<NodeId>,
}

/// Global manager for toast notifications.
pub struct Toaster<App> {
    pub active_toasts: VecDeque<Toast>,
    pub max_toasts: usize,
    _marker: std::marker::PhantomData<App>,
}

impl<App: 'static> Toaster<App> {
    pub fn new() -> Self {
        Self {
            active_toasts: VecDeque::new(),
            max_toasts: 5,
            _marker: std::marker::PhantomData,
        }
    }

    /// Primary entry point to show a toast, called via UiSystem.
    pub fn show_system(system: &mut UiSystem<App>, message: impl Into<String>, duration: f32) {
        let message = message.into();
        
        // Remove oldest if we hit the limit
        if system.toaster.active_toasts.len() >= system.toaster.max_toasts {
            if let Some(old) = system.toaster.active_toasts.pop_front() {
                if let Some(id) = old.node_id {
                    system.tree.remove_node(id);
                }
            }
        }

        let toast = Toast {
            message,
            duration,
            timer: duration,
            node_id: None,
        };

        system.toaster.active_toasts.push_back(toast);
        Self::rebuild_toasts_system(system);
    }

    pub fn update_system(system: &mut UiSystem<App>, dt: f32) {
        let mut expired = Vec::new();
        // Step 1: Update timers and collect expired ones
        for (i, toast) in system.toaster.active_toasts.iter_mut().enumerate() {
            toast.timer -= dt;
            if toast.timer <= 0.0 {
                expired.push(i);
            }
        }

        // Step 2: Remove expired toasts and their nodes
        if !expired.is_empty() {
            // Need to remove in reverse to preserve indices
            for i in expired.into_iter().rev() {
                if let Some(toast) = system.toaster.active_toasts.remove(i) {
                    if let Some(id) = toast.node_id {
                        system.tree.remove_node(id);
                    }
                }
            }
            Self::rebuild_toasts_system(system);
        }
    }

    fn rebuild_toasts_system(system: &mut UiSystem<App>) {
        // Clear existing nodes first to avoid re-borrowing conflict during panel spawn
        let mut ids_to_remove = Vec::new();
        for toast in &system.toaster.active_toasts {
            if let Some(id) = toast.node_id {
                ids_to_remove.push(id);
            }
        }
        for id in ids_to_remove {
            system.tree.remove_node(id);
        }

        // Rebuild each toast
        for i in 0..system.toaster.active_toasts.len() {
            let (msg, timer, duration) = {
                let toast = &system.toaster.active_toasts[i];
                (toast.message.clone(), toast.timer, toast.duration)
            };

            let timer_ratio = timer / duration;
            let opacity = (timer_ratio * 4.0).min(1.0); // Simple fade out

            let node_id = system.panel()
                .color(Color::from_rgba8(40, 40, 45, (opacity * 240.0) as u8))
                .border(Color::from_rgba8(100, 100, 110, (opacity * 255.0) as u8))
                .radius(8.0)
                .shadow(true)
                .padding(12.0)
                .at(20.0, 20.0 + (i as f32 * 60.0)) // For now, fixed at top-left
                .spawn(system);
                
            system.push_parent(node_id);
            system.label(msg)
                .color(Color::from_rgba8(240, 240, 255, (opacity * 255.0) as u8))
                .font_size(14.0)
                .spawn(system);
            system.pop_parent();

            // Update node_id in toaster
            if let Some(toast) = system.toaster.active_toasts.get_mut(i) {
                toast.node_id = Some(node_id);
            }
        }
    }
}

impl<App: 'static> Default for Toaster<App> {
    fn default() -> Self {
        Self::new()
    }
}
