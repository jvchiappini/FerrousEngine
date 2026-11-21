//! Per-pass GPU timings with `TIMESTAMP_QUERY`.
//!
//! The renderer writes a timestamp at the beginning and end of every
//! instrumented pass into a `QuerySet`; at the end of the frame it resolves the
//! set into a buffer and copies it to a mappable buffer.  The map is polled a
//! few frames later, so the CPU never stalls on the GPU.  When the adapter does
//! not expose `TIMESTAMP_QUERY`, [`GpuTimings::supported`] is false and the
//! summary reports `unsupported`; the renderer keeps working.
//!
//! Only passes whose descriptor accepts `timestamp_writes` are instrumented.
//! The non-layered Pure2D path writes `Pure2D Main Pass` and the GUI path
//! writes `GUI Render Pass`.  The layered Pure2D path reserves the labels
//! `Pure2D Layered Prefix`, `Pure2D Layered Main`, `Pure2D Plan`,
//! `Pure2D Backdrop Composite`, `Pure2D Mask`, `Pure2D Blur Effect` and
//! `Pure2D Effect Composite`; each label appears once its pass descriptor
//! supplies `timestamp_writes`.  A pass that is not instrumented is named in
//! the report as unsupported by the caller.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

/// Maximum number of instrumented passes per frame (two queries each).
///
/// The budget covers every timestampable pass a frame can encode without
/// quietly dropping labels:
///
/// | Label | Count per frame |
/// |-------|-----------------|
/// | `Pure2D Layered Prefix Pass` | 1 |
/// | `Pure2D Layered Main Pass` | 1 |
/// | `Pure2D Main Pass` (non-layered) | 1 |
/// | `Pure2D Plan Pass` | up to 8 |
/// | `Pure2D Backdrop Composite Pass` | up to 8 |
/// | `Pure2D Mask Pass` | up to `MAX_EFFECTS_PER_LAYER` |
/// | `Pure2D Blur Effect Pass` | 2 per effect, per layer |
/// | `Pure2D Effect Composite Pass` | 1 per effect, per layer |
/// | `GUI Render Pass` | 1 |
///
/// A layered frame is dominated by blur (two passes per effect) and effect
/// composites, so the old budget of 8 reserved its two queries for the main
/// and GUI passes and left every layer/effect pass uninstrumented.  Twenty-four
/// entries (48 queries at 16 bytes each) fit comfortably within typical
/// adapter query-set limits while covering a practical layer/effect graph.  A pass
/// beyond the budget still renders; `pass` returns `None` and only its timing
/// is missing.
const MAX_PASSES: u32 = 24;

/// A map result plus the pass labels/query indices it belongs to.
type PendingReadback = (
    Receiver<Result<(), wgpu::BufferAsyncError>>,
    Vec<(String, u32, u32)>,
);

/// The timestamp writes for one render pass, held alive until the pass ends.
#[derive(Debug, Clone)]
pub struct PassTiming {
    query_set: Arc<wgpu::QuerySet>,
    begin: u32,
    end: u32,
}

impl PassTiming {
    /// The descriptor fragment to pass to `RenderPassDescriptor::timestamp_writes`.
    #[must_use]
    pub fn writes(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(self.begin),
            end_of_pass_write_index: Some(self.end),
        }
    }
}

/// The per-renderer timestamp ring.
pub struct GpuTimings {
    supported: bool,
    query_set: Option<Arc<wgpu::QuerySet>>,
    resolve: Option<wgpu::Buffer>,
    readback: Option<wgpu::Buffer>,
    period_ns: f32,
    next_query: u32,
    labels: Vec<(String, u32, u32)>,
    pending: Option<PendingReadback>,
    frame_open: bool,
    samples: HashMap<String, Vec<f64>>,
}

impl std::fmt::Debug for GpuTimings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GpuTimings")
            .field("supported", &self.supported)
            .field("period_ns", &self.period_ns)
            .field("pending", &self.pending.is_some())
            .finish()
    }
}

impl GpuTimings {
    /// A renderer that never timestamps.  The engine runner calls
    /// [`crate::Renderer::enable_gpu_timings`] when the frame trace is on; a
    /// headless harness leaves it disabled even if the process has the trace
    /// environment variable set (the export path).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            supported: false,
            query_set: None,
            resolve: None,
            readback: None,
            period_ns: 1.0,
            next_query: 0,
            labels: Vec::new(),
            pending: None,
            frame_open: false,
            samples: HashMap::new(),
        }
    }

    /// Creates the query set when the device exposes `TIMESTAMP_QUERY`.
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // Timestamps are only useful when the frame trace is on, and creating
        // a query set costs driver work.  Gating on the trace also keeps the
        // headless tests (no trace) free of query-set setup on software
        // adapters whose timestamp support is unreliable.
        let supported = ferrous_core::frame_trace::enabled()
            && device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        if !supported {
            return Self {
                supported: false,
                query_set: None,
                resolve: None,
                readback: None,
                period_ns: 1.0,
                next_query: 0,
                labels: Vec::new(),
                pending: None,
                frame_open: false,
                samples: HashMap::new(),
            };
        }
        let count = MAX_PASSES * 2;
        let query_set = Arc::new(device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("Frame pass timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count,
        }));
        let size = u64::from(count) * std::mem::size_of::<u64>() as u64;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame pass timestamp resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame pass timestamp readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            supported: true,
            query_set: Some(query_set),
            resolve: Some(resolve),
            readback: Some(readback),
            period_ns: queue.get_timestamp_period(),
            next_query: 0,
            labels: Vec::new(),
            pending: None,
            frame_open: false,
            samples: HashMap::new(),
        }
    }

    /// Whether the adapter exposes timestamp queries.
    #[must_use]
    pub fn supported(&self) -> bool {
        self.supported
    }

    /// Opens a timing frame unless the previous readback is still pending.
    pub fn begin_frame(&mut self) {
        self.frame_open = self.supported && self.pending.is_none();
        self.next_query = 0;
        self.labels.clear();
    }

    /// Reserves a begin/end pair for `label`, or `None` when unavailable.
    pub fn pass(&mut self, label: &str) -> Option<PassTiming> {
        if !self.frame_open || self.next_query + 2 > MAX_PASSES * 2 {
            return None;
        }
        let begin = self.next_query;
        let end = begin + 1;
        self.next_query += 2;
        self.labels.push((label.to_owned(), begin, end));
        Some(PassTiming {
            query_set: Arc::clone(self.query_set.as_ref()?),
            begin,
            end,
        })
    }

    /// Resolves and copies the queries into the readback buffer.
    pub fn end_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.frame_open || self.next_query == 0 {
            return;
        }
        self.frame_open = false;
        let (Some(query_set), Some(resolve), Some(readback)) = (
            self.query_set.as_ref(),
            self.resolve.as_ref(),
            self.readback.as_ref(),
        ) else {
            return;
        };
        let size = u64::from(self.next_query) * std::mem::size_of::<u64>() as u64;
        encoder.resolve_query_set(query_set, 0..self.next_query, resolve, 0);
        encoder.copy_buffer_to_buffer(resolve, 0, readback, 0, size);
    }

    /// Starts the non-blocking map.  Call **after** the queue submit.
    pub fn after_submit(&mut self) {
        if self.labels.is_empty() || self.pending.is_some() {
            return;
        }
        let Some(readback) = self.readback.as_ref() else {
            return;
        };
        let slice = readback.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.pending = Some((receiver, self.labels.clone()));
    }

    /// Polls the device and consumes a finished readback.  Never blocks.
    pub fn poll(&mut self, device: &wgpu::Device) {
        let Some((receiver, labels)) = self.pending.take() else {
            return;
        };
        device.poll(wgpu::Maintain::Poll);
        match receiver.try_recv() {
            Ok(result) => {
                if result.is_ok() {
                    self.read(&labels);
                }
                if let Some(readback) = self.readback.as_ref() {
                    readback.unmap();
                }
            }
            Err(TryRecvError::Empty) => {
                self.pending = Some((receiver, labels));
            }
            Err(TryRecvError::Disconnected) => {
                if let Some(readback) = self.readback.as_ref() {
                    readback.unmap();
                }
            }
        }
    }

    fn read(&mut self, labels: &[(String, u32, u32)]) {
        let Some(readback) = self.readback.as_ref() else {
            return;
        };
        let mapped = readback.slice(..).get_mapped_range();
        let ticks: Vec<u64> = mapped
            .chunks_exact(std::mem::size_of::<u64>())
            .map(|chunk| {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(chunk);
                u64::from_le_bytes(bytes)
            })
            .collect();
        drop(mapped);
        for (label, begin, end) in labels {
            let (Some(&start), Some(&stop)) =
                (ticks.get(*begin as usize), ticks.get(*end as usize))
            else {
                continue;
            };
            let millis = stop.saturating_sub(start) as f64 * f64::from(self.period_ns) / 1.0e6;
            self.samples.entry(label.clone()).or_default().push(millis);
        }
    }

    /// Median GPU milliseconds per pass over every sampled frame, plus the
    /// unsupported marker when the adapter cannot timestamp.
    #[must_use]
    pub fn summary(&self) -> (Vec<(String, f64)>, Vec<String>) {
        if !self.supported {
            return (
                Vec::new(),
                vec!["TIMESTAMP_QUERY is not supported by the adapter".to_owned()],
            );
        }
        let mut passes: Vec<(String, f64)> = self
            .samples
            .iter()
            .map(|(label, values)| (label.clone(), median(values)))
            .collect();
        passes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        (passes, Vec::new())
    }

    /// Clears collected samples (tests / a new measurement window).
    pub fn reset(&mut self) {
        self.samples.clear();
        self.pending = None;
        self.labels.clear();
        self.next_query = 0;
        self.frame_open = false;
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) * 0.5
    } else {
        sorted[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::median;

    #[test]
    fn median_is_stable_for_odd_and_even_samples() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&[]), 0.0);
    }
}
