use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;
use core::any::Any;

/// Represents the data type of an inspectable property.
#[derive(Debug, Clone, PartialEq)]
pub enum PropType {
    Bool,
    I32,
    F32,
    String,
    Color,
    Vec2,
    Enum(Vec<String>),
    /// A nested logical structure (e.g. padding: { top, right, bottom, left })
    Struct(Vec<InspectorProp>),
}

/// A fully type-erased value passing through the property boundaries.
pub trait PropValue: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_value(&self) -> Box<dyn PropValue>;
}

/// Represents a single reflected property inside the visual editor UI.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectorProp {
    pub name: String,
    pub prop_type: PropType,
    // Note: in a fully reactive system, these could be Observables
    // but here we keep them as functional closures to fetch/update the backing data.
}

impl InspectorProp {
    pub fn new(name: impl Into<String>, prop_type: PropType) -> Self {
        Self {
            name: name.into(),
            prop_type,
        }
    }
}
