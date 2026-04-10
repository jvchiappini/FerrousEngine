use std::string::String;
use std::vec::Vec;
use crate::{NodeId, EventContext, UiEvent, EventResponse};
use core::any::Any;

/// Trait implemented via #[derive(FerrousController)].
/// Represents a user controller that manages a FUI view.
pub trait FerrousController: Any + 'static {
    /// Injects a NodeId generated from the FUI loader into the controller field labeled with #[fui_id].
    fn inject_fui_id(&mut self, id: &str, node: NodeId) -> bool;
    
    /// Routes an action emitted by a widget to a controller method labeled with #[fui_action].
    fn dispatch_fui_action(&mut self, action: &str, ctx: &mut EventContext<Self>, event: &UiEvent) -> EventResponse where Self: Sized;
    
    /// Optionally return the FUI structure as string if we baked it into the binary via include_str!
    fn static_fui_view(&self) -> Option<&'static str> {
        None
    }
}
