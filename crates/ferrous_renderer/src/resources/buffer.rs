/// Thin wrappers over `wgpu::Buffer` creation that enforce common usage
/// patterns and remove boilerplate from higher-level modules.
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Creates a GPU uniform buffer initialised with `data` and returns it
/// wrapped in an `Arc` so ownership can be shared between CPU-side code and
/// the bind groups that reference it.
///
/// The buffer is created with `UNIFORM | COPY_DST` usages, which is the
/// correct combination for a uniform that will be updated each frame.
pub fn create_uniform<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &str,
    data: &T,
) -> Arc<wgpu::Buffer> {
    Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::bytes_of(data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }),
    )
}

/// Creates a GPU vertex buffer from a slice of `Pod` data.
pub fn create_vertex<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &str,
    data: &[T],
) -> Arc<wgpu::Buffer> {
    Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    )
}

/// Creates a GPU index buffer from a slice of `Pod` data (typically `u16` or `u32`).
pub fn create_index<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &str,
    data: &[T],
) -> Arc<wgpu::Buffer> {
    Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::INDEX,
        }),
    )
}

/// Writes `data` to an existing uniform buffer.
///
/// Panics in debug builds if the buffer is not large enough to hold `T`.
pub fn update_uniform<T: bytemuck::Pod>(queue: &wgpu::Queue, buffer: &wgpu::Buffer, data: &T) {
    queue.write_buffer(buffer, 0, bytemuck::bytes_of(data));
}
