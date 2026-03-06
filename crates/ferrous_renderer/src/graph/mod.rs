// `graph` is now a thin re-export layer — all types live in `ferrous_render_graph`.
pub use ferrous_render_graph::{
    CameraPacket, FramePacket, InstancedDrawCommand, RenderPass, Viewport,
};

/// Sub-module aliases so that existing `use crate::graph::frame_packet::*` paths still resolve.
pub mod frame_packet {
    pub use ferrous_render_graph::{CameraPacket, FramePacket, InstancedDrawCommand, Viewport};
}

/// Sub-module alias so that `use crate::graph::pass_trait::RenderPass` still resolves.
pub mod pass_trait {
    pub use ferrous_render_graph::RenderPass;
}
