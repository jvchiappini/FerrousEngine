#![no_std]
//! A lightweight, zero-cost abstraction for type-erased widget properties 
//! and runtime reflection. Primarily designed to power `gui_maker` editor.

extern crate alloc;

pub mod prop;
pub mod reflect;

pub use prop::{InspectorProp, PropType, PropValue};
pub use reflect::Reflect;
