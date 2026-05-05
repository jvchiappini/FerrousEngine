use wgpu::{Buffer, BufferDescriptor, BufferUsages, CommandEncoder, Device, MapMode, Texture, Origin3d, TextureAspect, ImageCopyTexture, ImageCopyBuffer, ImageDataLayout, Extent3d, Maintain};

/// Allocates and manages a pinned GPU-to-CPU staging buffer for downloading frame pixels.
pub struct ReadbackFrameManager {
    pub buffer: Buffer,
    pub width: u32,
    pub height: u32,
}

impl ReadbackFrameManager {
    /// Creates a new buffer adapted to read back pixels of the given dimensions.
    /// In `wgpu`, when calling `copy_texture_to_buffer`, the `bytes_per_row` must be
    /// a multiple of 256. This is enforced tightly. The unpadded width is `width * 4` (RGBA8).
    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let align_size = 256;
        let unpadded_bytes = width * 4; // Assuming RGBA8
        let padded_bytes = ((unpadded_bytes + align_size - 1) / align_size) * align_size;
        
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Frame Readback Buffer"),
            size: (padded_bytes * height) as wgpu::BufferAddress,
            // We use COPY_DST so it can receive data from textures, 
            // and MAP_READ so the CPU can poll and read its content.
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            width,
            height,
        }
    }

    /// Queues a command in the provided encoder to copy the given texture into the readback buffer.
    pub fn copy_to_buffer(&self, encoder: &mut CommandEncoder, texture: &Texture) {
        let align_size = 256;
        let unpadded_bytes = self.width * 4;
        let padded_bytes = ((unpadded_bytes + align_size - 1) / align_size) * align_size;

        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &self.buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes),
                    rows_per_image: Some(self.height),
                },
            },
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Maps the buffer asynchronously and polls `wgpu` to force the transaction right now blocking.
    /// Emits the unwrapped packed RGBA buffer ready to be streamed or saved as an image.
    pub async fn poll_and_map(&self, device: &Device) -> Result<Vec<u8>, ()> {
        let slice = self.buffer.slice(..);
        
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| {
            if result.is_ok() {
                let _ = tx.send(());
            } else {
                let _ = tx.send(()); // Despertarlo incluso si hay error 
            }
        });

        // Ensure we force sync to effectively map instantly in a headless export mode scenarios
        device.poll(Maintain::Wait);

        if rx.recv().is_ok() {
            let data = slice.get_mapped_range();
            let mut result = Vec::with_capacity((self.width * self.height * 4) as usize);
            
            // To be robust against wgpu padding constraints:
            let align_size = 256;
            let unpadded_bytes = self.width * 4;
            let padded_bytes = ((unpadded_bytes + align_size - 1) / align_size) * align_size;
            
            for row in 0..self.height {
                let start = (row * padded_bytes) as usize;
                let end = start + unpadded_bytes as usize;
                result.extend_from_slice(&data[start..end]);
            }
            
            drop(data);
            self.buffer.unmap();
            Ok(result)
        } else {
            Err(())
        }
    }
}
