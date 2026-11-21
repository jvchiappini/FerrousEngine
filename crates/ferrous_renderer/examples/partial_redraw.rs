//! P5 item 3 — headless A/B for a persistent UI target with damaged-rect copy.
//!
//! Compares, at 1600x900 and (optionally) 3840x2160:
//!
//! * **full redraw** — a clear plus a synthetic pass that writes every pixel;
//! * **persistent path** — the same synthetic pass writes only a small damage
//!   rect into a [`PersistentUiTarget`] texture, then `copy_damage` copies just
//!   that rect to the present target.
//!
//! Each iteration encodes both paths into one command buffer (separate
//! textures), submits, and resolves timestamp queries.  The numbers are the
//! median GPU milliseconds per path and the bytes each path writes/copies, so
//! the break-even can be computed instead of guessed.
//!
//! Run from the engine repository root:
//!
//! ```text
//! cargo run --release -p ferrous_renderer --example partial_redraw
//! cargo run --release -p ferrous_renderer --example partial_redraw -- --4k
//! ```
//!
//! `--4k` asks for the 3840x2160 case even when the adapter's
//! `max_texture_dimension_2d` looks tight; the case is still skipped if the
//! device cannot allocate it, and the run stays at 1600x900 otherwise.  Without
//! the flag the 4K case runs when the device limit allows it.  Each size drops
//! both of its targets (`drop(full)` / `drop(present)`) before `measure`
//! returns, so only one resolution is resident: 4K RGBA8 is ~33 MB and the
//! persistent texture is another ~33 MB, never both sizes at once.
//!
//! `TIMESTAMP_QUERY` is required for the pass timings; without it the example
//! prints `unsupported` and exits cleanly.  Copy timing additionally needs
//! `TIMESTAMP_QUERY_INSIDE_ENCODERS`; when the adapter lacks it, the copy is
//! still encoded (so the bytes figure is real) but its milliseconds are printed
//! as `unsupported`.
//!
//! # What the numbers mean
//!
//! * `full_bytes = W * H * 4` (colour written by the pass and cleared).
//! * `damaged_bytes = dw * dh * 4` written by the pass plus the same amount
//!   copied to the present target.
//! * The persistent path wins when its measured GPU ms is lower.  It cannot win
//!   when the damage covers (nearly) the whole target, because the copy then
//!   adds bandwidth on top of a full repaint.  The 2-D editor repaints its whole
//!   chrome every frame, so today its damage *is* the window; this example
//!
//! This quantifies what a real damage source would have to beat.

#[cfg(feature = "gui")]
mod imp {

    use ferrous_ui_core::Rect;
    use ferrous_ui_render::PersistentUiTarget;
    use std::time::Instant;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
    const FRAMES: usize = 24;
    const WARMUP: usize = 3;
    /// Damage used for the persistent path; 320x180 is a large editor panel.
    const DAMAGE_W: u32 = 320;
    const DAMAGE_H: u32 = 180;
    const DAMAGE_X: u32 = 100;
    const DAMAGE_Y: u32 = 80;

    /// Timestamp pairs per frame: the full pass and the persistent pass always, the
    /// copy only when encoder-scope timestamps are available.
    const QUERIES_WITH_COPY: u32 = 6;
    const QUERIES_NO_COPY: u32 = 4;

    struct Gpu {
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_info: wgpu::AdapterInfo,
        timestamp_period: f32,
        timestamps: bool,
        encoder_timestamps: bool,
    }

    fn gpu() -> Option<Gpu> {
        let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        let adapter_info = adapter.get_info();
        let available = adapter.features();
        let mut required = available & wgpu::Features::TIMESTAMP_QUERY;
        if available.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS) {
            required |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("partial_redraw device"),
                required_features: required,
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        ))
        .ok()?;
        let timestamp_period = queue.get_timestamp_period();
        let timestamps = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let encoder_timestamps = device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        Some(Gpu {
            device,
            queue,
            adapter_info,
            timestamp_period,
            timestamps,
            encoder_timestamps,
        })
    }

    /// The draw pipeline: a full-screen triangle that does a fixed amount of work
    /// per fragment and writes one premultiplied colour with normal alpha blending.
    fn heavy_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("partial_redraw shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("partial_redraw.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("partial_redraw layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("partial_redraw pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    fn make_target(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        usage: wgpu::TextureUsages,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("partial_redraw target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        values[values.len() / 2]
    }

    #[derive(Default)]
    struct Timings {
        full: Vec<f64>,
        partial: Vec<f64>,
        copy: Vec<f64>,
        bytes_full: u64,
        bytes_partial: u64,
        bytes_copied: u64,
        copies: u32,
    }

    fn read_timestamps(
        device: &wgpu::Device,
        readback: &wgpu::Buffer,
        count: u32,
    ) -> Option<Vec<u64>> {
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        device.poll(wgpu::Maintain::Wait);
        receiver.recv().ok()?.ok()?;
        let mapped = readback.slice(..).get_mapped_range();
        let ticks: Vec<u64> = mapped
            .chunks_exact(std::mem::size_of::<u64>())
            .map(|chunk| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(chunk);
                u64::from_le_bytes(bytes)
            })
            .take(count as usize)
            .collect();
        drop(mapped);
        readback.unmap();
        Some(ticks)
    }

    fn measure(gpu: &Gpu, width: u32, height: u32, pipeline: &wgpu::RenderPipeline) -> Timings {
        let (present, present_view) = make_target(
            &gpu.device,
            width,
            height,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
        );
        let (full, full_view) = make_target(
            &gpu.device,
            width,
            height,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
        );
        let mut persist = PersistentUiTarget::new();
        persist.ensure(&gpu.device, width, height, FORMAT);
        persist.invalidate(Rect::new(
            DAMAGE_X as f32,
            DAMAGE_Y as f32,
            DAMAGE_W as f32,
            DAMAGE_H as f32,
        ));

        let Some(query_set) = (gpu.timestamps).then(|| {
            let count = if gpu.encoder_timestamps {
                QUERIES_WITH_COPY
            } else {
                QUERIES_NO_COPY
            };
            gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("partial_redraw timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count,
            })
        }) else {
            return Timings::default();
        };
        let query_count = if gpu.encoder_timestamps {
            QUERIES_WITH_COPY
        } else {
            QUERIES_NO_COPY
        };
        let bytes = u64::from(query_count) * std::mem::size_of::<u64>() as u64;
        let resolve = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("partial_redraw resolve"),
            size: bytes,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("partial_redraw readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // The first `ensure` allocates and leaves everything invalidated; use that
        // full-target damage as the seeding copy.
        persist.ensure(&gpu.device, width, height, FORMAT);
        let mut timings = Timings {
            bytes_full: u64::from(width) * u64::from(height) * 4,
            bytes_partial: u64::from(DAMAGE_W) * u64::from(DAMAGE_H) * 4,
            ..Default::default()
        };

        for frame in 0..(WARMUP + FRAMES) {
            if frame == 0 {
                // `ensure` already invalidated everything; take it as-is.
            } else {
                persist.invalidate(Rect::new(
                    DAMAGE_X as f32,
                    DAMAGE_Y as f32,
                    DAMAGE_W as f32,
                    DAMAGE_H as f32,
                ));
            }
            let damage = persist.take_damage();
            if timings.bytes_copied == 0 {
                timings.copies = u32::try_from(damage.len()).unwrap_or(u32::MAX);
            }

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("partial_redraw encoder"),
                });

            // (a) full redraw: clear + a pass that writes every pixel.
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("full redraw"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &full_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.10,
                                g: 0.12,
                                b: 0.16,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                        query_set: &query_set,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }),
                });
                pass.set_pipeline(pipeline);
                pass.draw(0..3, 0..1);
            }

            // (b) persistent path: the same pass, but only into the damaged rect.
            if let Some(view) = persist.view() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("persistent damaged rect"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if frame == 0 {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                        query_set: &query_set,
                        beginning_of_pass_write_index: Some(2),
                        end_of_pass_write_index: Some(3),
                    }),
                });
                pass.set_pipeline(pipeline);
                pass.set_scissor_rect(
                    DAMAGE_X,
                    DAMAGE_Y,
                    DAMAGE_W.min(width.saturating_sub(DAMAGE_X)),
                    DAMAGE_H.min(height.saturating_sub(DAMAGE_Y)),
                );
                pass.draw(0..3, 0..1);
            }

            // (c) copy the damaged rect from the persistent texture into the
            // present target.
            if gpu.encoder_timestamps {
                encoder.write_timestamp(&query_set, 4);
            }
            let copy = persist.copy_damage(&mut encoder, &present, &present_view);
            if gpu.encoder_timestamps {
                encoder.write_timestamp(&query_set, 5);
            }

            encoder.resolve_query_set(&query_set, 0..query_count, &resolve, 0);
            encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, bytes);
            gpu.queue.submit(Some(encoder.finish()));

            if frame < WARMUP {
                // Keep the GPU from running too far ahead during warm-up.
                gpu.device.poll(wgpu::Maintain::Wait);
                continue;
            }
            let Some(ticks) = read_timestamps(&gpu.device, &readback, query_count) else {
                continue;
            };
            let to_ms = |index: usize| {
                ticks
                    .get(index)
                    .map(|&tick| tick as f64 * f64::from(gpu.timestamp_period) / 1.0e6)
            };
            if let (Some(begin), Some(end)) = (to_ms(0), to_ms(1)) {
                timings.full.push(end - begin);
            }
            if let (Some(begin), Some(end)) = (to_ms(2), to_ms(3)) {
                timings.partial.push(end - begin);
            }
            if let (Some(begin), Some(end)) = (to_ms(4), to_ms(5)) {
                timings.copy.push(end - begin);
            }
            if timings.bytes_copied == 0 {
                timings.bytes_copied = copy.bytes;
                timings.copies = copy.rects;
            }
        }

        drop(full);
        drop(present);
        persist.ensure(&gpu.device, 0, 0, FORMAT);
        timings
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let force_4k = args.iter().any(|arg| arg == "--4k");
        let Some(gpu) = gpu() else {
            eprintln!("no GPU adapter available");
            return;
        };
        println!(
            "adapter: {} ({:?}, {:?})",
            gpu.adapter_info.name, gpu.adapter_info.device_type, gpu.adapter_info.backend
        );
        println!("timestamp_period: {:.4} ns", gpu.timestamp_period);
        if !gpu.timestamps {
            println!("GPU timings: unsupported (adapter has no TIMESTAMP_QUERY)");
            return;
        }
        if !gpu.encoder_timestamps {
            println!("copy timing: unsupported (no TIMESTAMP_QUERY_INSIDE_ENCODERS)");
        }

        let pipeline = heavy_pipeline(&gpu.device);
        let max_dimension = gpu.device.limits().max_texture_dimension_2d;

        let sizes: Vec<(u32, u32)> = if force_4k || max_dimension >= 3840 {
            vec![(1600, 900), (3840, 2160)]
        } else {
            println!(
                "adapters max_texture_dimension_2d = {} < 3840; skipping 4K (use --4k to force)",
                max_dimension
            );
            vec![(1600, 900)]
        };

        for (width, height) in sizes {
            if width > max_dimension || height > max_dimension {
                println!(
                    "skipping {}x{}: above max_texture_dimension_2d",
                    width, height
                );
                continue;
            }
            let start = Instant::now();
            let mut timings = measure(&gpu, width, height, &pipeline);
            let wall = start.elapsed().as_secs_f64() * 1000.0;
            let full = median(&mut timings.full);
            let partial_render = median(&mut timings.partial);
            let copy = median(&mut timings.copy);
            let partial_total = if gpu.encoder_timestamps {
                partial_render + copy
            } else {
                partial_render
            };
            let window_pixels = u64::from(width) * u64::from(height);
            let damage_pixels = u64::from(DAMAGE_W) * u64::from(DAMAGE_H);
            println!(
                "\n=== {}x{} ({:.1} MB RGBA8 per full target) ===",
                width,
                height,
                timings.bytes_full as f64 / 1.0e6
            );
            println!(
                "full redraw      : {:8.3} ms GPU  ({:.2} MB written + cleared, every pixel)",
                full,
                timings.bytes_full as f64 / 1.0e6
            );
            println!(
                "persistent draw  : {:8.3} ms GPU  ({}x{} = {:.1}% of the window, {:.2} MB written)",
                partial_render,
                DAMAGE_W,
                DAMAGE_H,
                damage_pixels as f64 / window_pixels as f64 * 100.0,
                timings.bytes_partial as f64 / 1.0e6
            );
            if gpu.encoder_timestamps {
                println!(
                    "copy_damage      : {:8.3} ms GPU  ({} rect(s), {:.2} MB copied this frame)",
                    copy,
                    timings.copies,
                    timings.bytes_copied as f64 / 1.0e6
                );
            } else {
                println!(
                    "copy_damage      : unsupported     ({} rect(s), {:.2} MB copied)",
                    timings.copies,
                    timings.bytes_partial as f64 / 1.0e6
                );
            }
            println!(
                "partial total    : {:8.3} ms GPU  ({:.1}x less pixel work)",
                partial_total,
                timings.bytes_full as f64 / timings.bytes_partial.max(1) as f64
            );
            if full > 0.0 {
                println!(
                    "speedup vs full  : {:8.2}x  (full {full:.3} ms => partial {partial_total:.3} ms)",
                    full / partial_total.max(1.0e-6)
                );
            }
            // Break-even damaged fraction: the persistent path stops winning when
            // repair cost per pixel plus copy cost per pixel reaches the full
            // redraw cost per pixel, i.e. `damage / window = full_per_px / (render_per_px + copy_per_px)`.
            let full_per_pixel = full / window_pixels as f64;
            let copy_per_pixel = if gpu.encoder_timestamps {
                copy / damage_pixels as f64
            } else {
                0.0
            };
            let render_per_pixel = partial_render / damage_pixels.max(1) as f64;
            let break_even = if full_per_pixel > 0.0 {
                full_per_pixel / (render_per_pixel + copy_per_pixel).max(1.0e-12)
            } else {
                f64::NAN
            };
            println!(
                "break-even damage: {:.1}% of the window (from measured per-pixel rates)",
                break_even * 100.0
            );
            println!(
                "wall time        : {:.1} ms for {} frames",
                wall,
                WARMUP + FRAMES
            );
        }
    }
}

#[cfg(feature = "gui")]
fn main() {
    imp::main();
}

#[cfg(not(feature = "gui"))]
fn main() {
    println!("partial_redraw requires the `gui` feature (default) to build ferrous_ui_render");
}
