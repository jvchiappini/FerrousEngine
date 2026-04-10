use alloc::vec::Vec;
use crate::prop::InspectorProp;

/// A trait that allows structs (Widgets, Components) to expose their 
/// internal properties systematically for dynamic runtime editing and serialization.
pub trait Reflect {
    /// Returns the name of the component (e.g., "Button", "Panel")
    fn type_name(&self) -> &'static str;

    /// Retrieves all the editable properties to build dynamic UI panels.
    fn properties(&self) -> Vec<InspectorProp>;

    // We can also add dynamic get/set through Any downcasting
    // fn set_property(&mut self, name: &str, value: Box<dyn PropValue>) -> Result<(), String>;
}
