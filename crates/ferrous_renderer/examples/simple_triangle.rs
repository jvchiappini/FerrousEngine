// Ejemplo mínimo dentro del crate `ferrous_renderer` que construye el
// renderizador y emite un frame. Permite compilar el pipeline sin depender de
// una aplicación completa.

use ferrous_gui;

fn main() {
    pollster::block_on(async {
        let context = ferrous_core::context::EngineContext::new()
            .await
            .expect("failed to create context");

        let mut renderer = ferrous_renderer::Renderer::new(
            context,
            800,
            600,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );

        let mut encoder = renderer.begin_frame();
        // generamos un rectángulo de UI sobre la escena para demostrar
        // la composición de capas. La coordenada (50,50) es la esquina
        // superior izquierda en píxeles y el tamaño es 200x100.
        let mut ui_batch = ferrous_gui::GuiBatch::new();
        ui_batch.push(ferrous_gui::GuiQuad {
            pos: [50.0, 50.0],
            size: [200.0, 100.0],
            color: [0.0, 1.0, 0.0, 0.5],
        });
        renderer.render_to_target(&mut encoder, Some(&ui_batch));

        // además de enviar los comandos, copiamos el contenido de la textura
        // de color a un buffer CPU para poder guardarlo en disco y verificar
        // visualmente que el triángulo se ha dibujado.
        let width = 800u32;
        let height = 600u32;
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // bytes per row must be a multiple of COPY_BYTES_PER_ROW_ALIGNMENT
        let unaligned_bytes_per_row = 4 * width;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes_per_row = ((unaligned_bytes_per_row + align - 1) / align) * align;

        let output_buffer = renderer
            .context
            .device
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("Readback Buffer"),
                size: (bytes_per_row as u64 * height as u64),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &renderer.render_target.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            texture_size,
        );

        // submit and wait for the work to finish so the map will succeed
        renderer
            .context
            .queue
            .submit(std::iter::once(encoder.finish()));
        renderer
            .context
            .device
            .poll(wgpu::Maintain::Wait);

        // map the buffer and save PNG
        let buffer_slice = output_buffer.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        renderer
            .context
            .device
            .poll(wgpu::Maintain::Wait);
        let data = buffer_slice.get_mapped_range();

        // the texture format is BGRA; convert to RGBA for the PNG library
        let mut png_data = Vec::with_capacity((4 * width * height) as usize);
        // the buffer rows may be padded; we only copy the first `unaligned_bytes_per_row` of each row
        for row in 0..height as usize {
            let start = (row as u64 * bytes_per_row as u64) as usize;
            let row_bytes = &data[start..start + (4 * width) as usize];
            for chunk in row_bytes.chunks_exact(4) {
                png_data.push(chunk[2]); // R
                png_data.push(chunk[1]); // G
                png_data.push(chunk[0]); // B
                png_data.push(chunk[3]); // A
            }
        }

        image::save_buffer(
            "frame.png",
            &png_data,
            800,
            600,
            image::ColorType::Rgba8,
        )
        .expect("failed to save image");

        println!("Rendered a single triangle frame, output written to frame.png");
    });
}
