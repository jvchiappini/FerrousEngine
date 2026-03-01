/// Synchronises a `ferrous_core::scene::World` with the renderer's internal
/// object list.
///
/// This function is intentionally a free function (not a method) so that it
/// can be called from the `Renderer` without requiring `&mut self` access to
/// all renderer internals — only the object list and device are needed.
use ferrous_core::scene::{Element, World};

use crate::geometry::Mesh;
use crate::scene::RenderObject;

/// Ensures the renderer's `objects` list reflects the current state of `world`.
///
/// New cubes spawn a `RenderObject`; existing ones have their position
/// updated cheaply via a GPU `write_buffer`.
pub fn sync_world(
    world: &mut World,
    objects: &mut Vec<RenderObject>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    model_layout: &wgpu::BindGroupLayout,
) {
    // Collect a snapshot first to avoid simultaneous borrow of `world`.
    let entries: Vec<(usize, Element)> = world
        .elements_with_handles()
        .map(|(h, e)| (h, e.clone()))
        .collect();

    for (handle, elem) in entries {
        #[allow(irrefutable_let_patterns)]
        if let Element::Cube(cube) = elem {
            let pos = cube.position;
            if let Some(idx) = world.render_handle(handle) {
                if let Some(obj) = objects.get_mut(idx) {
                    obj.set_position(queue, pos);
                }
            } else {
                let mesh = Mesh::cube(device);
                let idx  = objects.len();
                objects.push(RenderObject::new(device, mesh, pos, model_layout));
                world.set_render_handle(handle, idx);
            }
        }
    }
}
