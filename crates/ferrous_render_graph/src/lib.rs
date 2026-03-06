//! `ferrous_render_graph` — `RenderPass` trait and `FramePacket` for FerrousEngine.
//!
//! This crate is intentionally minimal: it contains only the types that
//! external pass implementors need.  It has no dependency on `ferrous_renderer`
//! itself, so third-party passes can implement `RenderPass` without pulling in
//! the full renderer.

pub mod frame_packet;
pub mod pass_trait;

pub use frame_packet::{CameraPacket, FramePacket, InstancedDrawCommand, Viewport};
pub use pass_trait::RenderPass;
