use std::sync::Arc;
use ferrous_render_graph::{FramePacket, RenderPass};
use wgpu::{CommandEncoder, Device, Queue, TextureView};

use ferrous_2d::render::{Renderer2d, SpriteBatcher};
use glam::Mat4;

/// Generable and Integrable Sprite pass.
pub struct SpritePass {
    pub renderer_2d: Renderer2d,
    pub batcher: SpriteBatcher,
    
    // Bind group cache or lookup function could be passed from the main Renderer
    // For now we just keep a list or map of known BindGroups if we were managing them directly,
    // but typically textures are stored in the asset server or RenderResource map.
}

impl SpritePass {
    pub fn new(device: Arc<Device>, output_format: wgpu::TextureFormat) -> Self {
        Self {
            renderer_2d: Renderer2d::new(device, output_format, 1, 1024),
            batcher: SpriteBatcher::default(),
        }
    }


    /// Prepares by capturing the latest Z-sorted sprites from the ECS
    pub fn prepare_from_ecs(&mut self, ecs_world: &mut ferrous_ecs::world::World, queue: &Queue, proj_view: Mat4) {
        // Here we extract entities
        let query = ferrous_ecs::prelude::Query::<(&ferrous_2d::components::Transform2d, &ferrous_2d::components::Sprite)>::new(ecs_world);
        ferrous_2d::systems::prepare_sprites_system(&mut self.batcher, query);
        
        self.renderer_2d.update_camera(queue, proj_view);
        self.renderer_2d.prepare(queue, &self.batcher);
    }
}

// Since RenderPass executes the command encoder:
impl RenderPass for SpritePass {
    fn name(&self) -> &str {
        "Sprite 2D Pass"
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {
        // App handles ECS extracting and calling prepare_from_ecs()
    }

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        _packet: &FramePacket,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Sprite Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: resolve_target.unwrap_or(color_view),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // We are compositing on top of 3D scene (or clear color if first)
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_view.map(|dv| wgpu::RenderPassDepthStencilAttachment {
                view: dv,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // We get the texture BindGroups from the FramePacket/Renderer ctx.
        // For now, assume packet can supply a closure or mapping.
        // This is a placeholder for texture binding fetching.
        self.renderer_2d.render(&mut render_pass, &self.batcher, |_tex_id| {
            // TODO: Get actual BindGroup for texture from Asset/Resource Manager
            None
        });
    }
}
