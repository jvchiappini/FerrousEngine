//! The runner's window map: the main window plus owned popup windows.
//!
//! A popup is a borderless, owned window that never steals activation. The
//! map is the single source of truth for which windows exist, which are
//! visible, and where input for a `WindowId` must go. The actual OS window is
//! created by a [`PopupBackend`], so the map, the pooling and the routing are
//! testable headlessly with a mock backend.
//!
//! Hidden popups are **pooled**: a new popup reuses a hidden one (moving and
//! resizing it) instead of creating another OS window, which keeps the time to
//! a popup's first frame and its extra memory flat after the first open.
//!
//! Popup ids come from [`PopupQueue::open`]: the app reserves the id at once
//! and the runner creates the window later, in `about_to_wait`, where it has
//! the event loop. The map therefore keys its slots by that same id, so the id
//! an app holds never changes while the popup is open.
//!
//! All coordinates are physical pixels; the caller derives them from the
//! monitor work area and the `GUIMaker` placement solver.

use std::hash::{DefaultHasher, Hash, Hasher};

use winit::window::WindowId;

#[cfg(not(target_arch = "wasm32"))]
use ferrous_core::memory::{self, Subsystem};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

/// A popup's stable id, assigned by [`PopupQueue::open`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PopupId(pub u32);

/// How many hidden popup windows the pool keeps alive for reuse.
///
/// A hidden popup costs about 50 MB of RSS (a swapchain plus its surfaces), so
/// the pool is deliberately tiny: two covers the common "menu then submenu"
/// case without letting a long session accumulate windows. A third hide
/// destroys the oldest hidden one.
pub const POPUP_POOL_CAP: usize = 2;

/// Swapchain images assumed when estimating a popup's VRAM.
///
/// wgpu 23 has no query for the actual DXGI/Vulkan image count; a flip-model
/// swapchain with `desired_maximum_frame_latency: 1` usually allocates two or
/// three.  The report uses three as a documented upper-bound estimate.
pub const POPUP_SWAPCHAIN_IMAGES: u32 = 3;

/// Where an event for a window must be delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowTarget {
    /// The main window (the existing single-window path).
    Main,
    /// A popup owned by the main window.
    Popup(PopupId),
}

/// The geometry and styles a popup is created with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupSpec {
    /// Initial position, physical pixels.
    pub position: (i32, i32),
    /// Size, physical pixels.
    pub size: (u32, u32),
    /// Ask for an alpha-capable transparent window.
    pub transparent: bool,
    /// Whether the user can resize it.
    pub resizable: bool,
}

impl Default for PopupSpec {
    fn default() -> Self {
        Self {
            position: (0, 0),
            size: (200, 120),
            transparent: true,
            resizable: false,
        }
    }
}

/// One queued popup operation, drained by the runner where it has the event
/// loop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupCommand {
    /// Create (or reuse a pooled) window for `id`.
    Open {
        /// The id reserved by [`PopupQueue::open`].
        id: PopupId,
        /// Geometry and styles.
        spec: PopupSpec,
    },
    /// Show it at a physical position.
    Show {
        /// Target popup.
        id: PopupId,
        /// Physical position.
        position: (i32, i32),
    },
    /// Move it.
    Move {
        /// Target popup.
        id: PopupId,
        /// Physical position.
        position: (i32, i32),
    },
    /// Resize it.
    Resize {
        /// Target popup.
        id: PopupId,
        /// Physical size.
        size: (u32, u32),
    },
    /// Hide it (kept for pooling).
    Hide {
        /// Target popup.
        id: PopupId,
    },
    /// Destroy it.
    Close {
        /// Target popup.
        id: PopupId,
    },
    /// Ask the OS for one frame of that popup.
    Redraw {
        /// Target popup.
        id: PopupId,
    },
}

/// The app-facing command queue.
///
/// `open` reserves and returns a [`PopupId`] at once; the runner creates the
/// real window later, in `about_to_wait`, where it has the `ActiveEventLoop`.
/// A popup redraws only when something asks for it, so an idle popup presents
/// zero frames.
#[derive(Debug, Default)]
pub struct PopupQueue {
    next: u32,
    commands: Vec<PopupCommand>,
    /// A popup-frame capture was requested this frame.
    capture: bool,
}

impl PopupQueue {
    /// Builds an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves a popup id and queues its creation.
    pub fn open(&mut self, spec: PopupSpec) -> PopupId {
        let id = PopupId(self.next);
        self.next += 1;
        self.commands.push(PopupCommand::Open { id, spec });
        id
    }

    /// Shows `id` at `position`.
    pub fn show(&mut self, id: PopupId, position: (i32, i32)) {
        self.commands.push(PopupCommand::Show { id, position });
    }

    /// Moves `id`.
    pub fn move_to(&mut self, id: PopupId, position: (i32, i32)) {
        self.commands.push(PopupCommand::Move { id, position });
    }

    /// Resizes `id`.
    pub fn resize(&mut self, id: PopupId, size: (u32, u32)) {
        self.commands.push(PopupCommand::Resize { id, size });
    }

    /// Hides `id`, returning it to the pool.
    pub fn hide(&mut self, id: PopupId) {
        self.commands.push(PopupCommand::Hide { id });
    }

    /// Closes `id`.
    pub fn close(&mut self, id: PopupId) {
        self.commands.push(PopupCommand::Close { id });
    }

    /// Requests one presented frame for `id`.
    pub fn redraw(&mut self, id: PopupId) {
        self.commands.push(PopupCommand::Redraw { id });
    }

    /// Asks for one readback of every visible popup's next frame.
    ///
    /// The runner arms each popup and asks it for a frame; each popup then
    /// delivers its own surface through
    /// [`crate::traits::FerrousApp::on_popup_screenshot`]. Nothing is read from
    /// the desktop.
    pub fn capture_all(&mut self) {
        self.capture = true;
    }

    /// Takes the pending capture request.
    #[must_use]
    pub fn take_capture_request(&mut self) -> bool {
        std::mem::take(&mut self.capture)
    }

    /// Whether a command is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Takes the queued commands.
    #[must_use]
    pub fn drain(&mut self) -> Vec<PopupCommand> {
        std::mem::take(&mut self.commands)
    }
}

/// The OS window behind the map.
///
/// A backend that cannot create windows (a headless unit test, or a host
/// without popup support) returns `None` and the map degrades to "no popups".
pub trait PopupBackend {
    /// What `create` needs: the event loop on desktop, `()` in a test. Clone,
    /// because one drain may create more than one popup.
    type CreateContext<'a>: Clone;

    /// Creates a hidden window and returns its handle.
    fn create(&mut self, spec: PopupSpec, context: Self::CreateContext<'_>) -> Option<u32>;
    /// Sets visibility.
    fn set_visible(&mut self, handle: u32, visible: bool);
    /// Moves the window.
    fn set_position(&mut self, handle: u32, position: (i32, i32));
    /// Resizes the window.
    fn set_size(&mut self, handle: u32, size: (u32, u32));
    /// Destroys the window.
    fn destroy(&mut self, handle: u32);
    /// Whether the window is transparent in practice (the documented fallback
    /// is an opaque window that paints its own background).
    fn transparent(&self, handle: u32) -> bool;
    /// A routing key for a handle; `None` when the window is gone.
    fn key(&self, handle: u32) -> Option<u64>;
    /// Arms a readback of this popup's next presented frame (evidence capture).
    fn arm_capture(&mut self, _handle: u32) {}
    /// The routing key for an event's `WindowId`.
    fn key_of(&self, window: WindowId) -> u64 {
        key_of(window)
    }
    /// `id` was opened again on this window (a pooled popup, or a repeat
    /// `Open`): the time-to-first-frame clock restarts.
    fn reopened(&mut self, _handle: u32) {}
}

/// The routing key of a `WindowId` (stable for the process).
#[must_use]
pub fn key_of(window: WindowId) -> u64 {
    let mut hasher = DefaultHasher::new();
    window.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug)]
struct Entry {
    handle: u32,
    spec: PopupSpec,
    visible: bool,
    /// `hide` put it in the pool. A popup that was opened but never shown is
    /// not poolable: a second `open` in the same frame must not steal it.
    poolable: bool,
    key: u64,
}

/// The main window plus every popup, live or pooled.
pub struct WindowMap<B: PopupBackend> {
    main_key: u64,
    backend: B,
    /// Slot table: the index is the [`PopupId`] the app reserved, and an id
    /// never moves, so destroying or pooling one never renumbers the others.
    entries: Vec<Option<Entry>>,
    /// The main window is minimized or occluded: every logical popup is hidden
    /// at the OS level, and restoring re-shows the ones that were visible.
    occluded: bool,
}

impl<B: PopupBackend> WindowMap<B> {
    /// Builds a map over the main window's routing key.
    #[must_use]
    pub fn new(main: WindowId, backend: B) -> Self {
        Self {
            main_key: key_of(main),
            backend,
            entries: Vec::new(),
            occluded: false,
        }
    }

    /// The main window's routing key.
    #[must_use]
    pub fn main_key(&self) -> u64 {
        self.main_key
    }

    /// The OS backend.
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// The OS backend, mutably.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Opens the popup `id` reserved by [`PopupQueue::open`], reusing a hidden
    /// one when possible.
    ///
    /// The popup starts hidden. Returns `false` when the backend cannot create
    /// a window; the caller then keeps the overlay in the main window.
    pub fn open(&mut self, id: PopupId, spec: PopupSpec, context: B::CreateContext<'_>) -> bool {
        if self.entry(id).is_some() {
            if let Some(handle) = self.entry_mut(id).map(|entry| {
                entry.spec = spec;
                entry.handle
            }) {
                self.backend.set_position(handle, spec.position);
                self.backend.set_size(handle, spec.size);
                self.backend.reopened(handle);
            }
            return true;
        }
        // Reuse a hidden popup: its OS window keeps its handle, routing key and
        // surface; only the slot (the id the app holds) changes.
        if let Some(index) = self.entries.iter().position(|entry| {
            entry
                .as_ref()
                .is_some_and(|entry| !entry.visible && entry.poolable)
        }) {
            let Some(mut entry) = self.entries[index].take() else {
                return false;
            };
            entry.spec = spec;
            entry.visible = false;
            entry.poolable = false;
            self.backend.set_position(entry.handle, spec.position);
            self.backend.set_size(entry.handle, spec.size);
            self.backend.set_visible(entry.handle, false);
            self.backend.reopened(entry.handle);
            self.place(id, entry);
            return true;
        }
        let Some(handle) = self.backend.create(spec, context) else {
            return false;
        };
        let Some(key) = self.backend.key(handle) else {
            self.backend.destroy(handle);
            return false;
        };
        self.place(
            id,
            Entry {
                handle,
                spec,
                visible: false,
                poolable: false,
                key,
            },
        );
        true
    }

    /// Creates a popup at the next free id. Convenience over [`Self::open`]
    /// for callers that do not run a [`PopupQueue`].
    pub fn create(&mut self, spec: PopupSpec, context: B::CreateContext<'_>) -> Option<PopupId> {
        let id = self
            .entries
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.entries.len());
        let id = PopupId(u32::try_from(id).ok()?);
        self.open(id, spec, context).then_some(id)
    }

    /// Shows `id` at `position`.
    pub fn show(&mut self, id: PopupId, position: (i32, i32)) {
        let Some(handle) = self.entry_mut(id).map(|entry| {
            entry.spec.position = position;
            entry.visible = true;
            entry.poolable = false;
            entry.handle
        }) else {
            return;
        };
        self.backend.set_position(handle, position);
        self.backend.set_visible(handle, !self.occluded);
    }

    /// Moves `id`.
    pub fn move_to(&mut self, id: PopupId, position: (i32, i32)) {
        let Some(handle) = self.entry_mut(id).map(|entry| {
            entry.spec.position = position;
            entry.handle
        }) else {
            return;
        };
        self.backend.set_position(handle, position);
    }

    /// Resizes `id`.
    pub fn resize(&mut self, id: PopupId, size: (u32, u32)) {
        let Some(handle) = self.entry_mut(id).map(|entry| {
            entry.spec.size = size;
            entry.handle
        }) else {
            return;
        };
        self.backend.set_size(handle, size);
    }

    /// Hides `id`, returning it to the pool.
    ///
    /// The pool keeps at most [`POPUP_POOL_CAP`] hidden windows; hiding one
    /// beyond the cap destroys the oldest hidden window for good.
    pub fn hide(&mut self, id: PopupId) {
        let Some(handle) = self.entry_mut(id).map(|entry| {
            entry.visible = false;
            entry.poolable = true;
            entry.handle
        }) else {
            return;
        };
        self.backend.set_visible(handle, false);
        self.trim_pool();
    }

    /// Destroys the oldest hidden popup while the pool is over its cap.
    fn trim_pool(&mut self) {
        loop {
            let hidden: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    entry
                        .as_ref()
                        .filter(|entry| !entry.visible && entry.poolable)
                        .map(|_| index)
                })
                .collect();
            if hidden.len() <= POPUP_POOL_CAP {
                return;
            }
            let Some(index) = hidden.first().copied() else {
                return;
            };
            if let Some(Some(entry)) = self.entries.get_mut(index).map(Option::take) {
                self.backend.destroy(entry.handle);
            }
        }
    }

    /// Destroys `id` for good.
    pub fn destroy(&mut self, id: PopupId) {
        if let Some(slot) = self.entries.get_mut(id.0 as usize) {
            if let Some(entry) = slot.take() {
                self.backend.destroy(entry.handle);
            }
        }
    }

    /// Whether `id` exists.
    #[must_use]
    pub fn contains(&self, id: PopupId) -> bool {
        self.entry(id).is_some()
    }

    /// Whether `id` is visible.
    #[must_use]
    pub fn visible(&self, id: PopupId) -> bool {
        self.entry(id).is_some_and(|entry| entry.visible)
    }

    /// The OS handle behind `id`.
    #[must_use]
    pub fn handle(&self, id: PopupId) -> Option<u32> {
        self.entry(id).map(|entry| entry.handle)
    }

    /// The spec `id` was last created or moved with.
    #[must_use]
    pub fn spec(&self, id: PopupId) -> Option<PopupSpec> {
        self.entry(id).map(|entry| entry.spec)
    }

    /// Whether `id`'s window is actually transparent.
    #[must_use]
    pub fn transparent(&self, id: PopupId) -> bool {
        self.entry(id)
            .is_some_and(|entry| self.backend.transparent(entry.handle))
    }

    /// The number of live popup entries (visible or pooled).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.iter().flatten().count()
    }

    /// Whether there are no popups.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Hides every popup while the main window is minimized or occluded, and
    /// re-shows the visible ones on restore.
    ///
    /// The logical visibility is untouched, so the app never has to re-open an
    /// overlay because the user minimized the window.
    pub fn set_occluded(&mut self, occluded: bool) {
        if self.occluded == occluded {
            return;
        }
        self.occluded = occluded;
        let handles: Vec<(u32, bool)> = self
            .entries
            .iter()
            .flatten()
            .map(|entry| (entry.handle, entry.visible))
            .collect();
        for (handle, visible) in handles {
            self.backend.set_visible(handle, visible && !occluded);
        }
    }

    /// Moves every visible popup by `(dx, dy)` physical pixels.
    ///
    /// The main window moved: shifting the popups in the same callback keeps
    /// them glued to their anchors instead of waiting for the next frame. The
    /// next frame re-solves the work area and corrects any clamp.
    pub fn shift_visible(&mut self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        let positions: Vec<(u32, (i32, i32))> = self
            .entries
            .iter_mut()
            .flatten()
            .filter(|entry| entry.visible)
            .map(|entry| {
                entry.spec.position = (
                    entry.spec.position.0.saturating_add(dx),
                    entry.spec.position.1.saturating_add(dy),
                );
                (entry.handle, entry.spec.position)
            })
            .collect();
        for (handle, position) in positions {
            self.backend.set_position(handle, position);
        }
    }

    /// Arms a readback of every visible popup's next frame.
    ///
    /// Returns the handles that were armed so the runner can ask them for a
    /// frame; the per-popup present path delivers the pixels.
    pub fn arm_capture(&mut self) -> Vec<u32> {
        let handles: Vec<u32> = self
            .entries
            .iter()
            .flatten()
            .filter(|entry| entry.visible)
            .map(|entry| entry.handle)
            .collect();
        for &handle in &handles {
            self.backend.arm_capture(handle);
        }
        handles
    }

    /// Where an event for `window` goes.
    #[must_use]
    pub fn route(&self, window: WindowId) -> WindowTarget {
        self.route_key(self.backend.key_of(window))
    }

    /// [`Self::route`] by routing key (the pure half, used by tests).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // reason: one slot per popup, far below u32::MAX
    pub fn route_key(&self, key: u64) -> WindowTarget {
        if key == self.main_key {
            return WindowTarget::Main;
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.as_ref().is_some_and(|entry| entry.key == key) {
                return WindowTarget::Popup(PopupId(index as u32));
            }
        }
        WindowTarget::Main
    }

    fn place(&mut self, id: PopupId, entry: Entry) {
        let index = id.0 as usize;
        if self.entries.len() <= index {
            self.entries.resize_with(index + 1, || None);
        }
        self.entries[index] = Some(entry);
    }

    fn entry(&self, id: PopupId) -> Option<&Entry> {
        self.entries.get(id.0 as usize).and_then(Option::as_ref)
    }

    fn entry_mut(&mut self, id: PopupId) -> Option<&mut Entry> {
        self.entries.get_mut(id.0 as usize).and_then(Option::as_mut)
    }
}

/// Applies every queued command to `map`, returning the popups that must be
/// redrawn.
///
/// The runner calls this in `about_to_wait`: the only callback with the
/// event loop the backend needs to create a window. An `Open`, `Show`, `Move`
/// or `Resize` asks for one frame too, so a new popup appears with its
/// content instead of an empty surface.
#[allow(clippy::needless_pass_by_value)] // reason: one context is cloned per Open in the drain
pub fn drain<B: PopupBackend>(
    map: &mut WindowMap<B>,
    queue: &mut PopupQueue,
    context: B::CreateContext<'_>,
) -> Vec<PopupId> {
    let mut redraw = Vec::new();
    for command in queue.drain() {
        match command {
            PopupCommand::Open { id, spec } => {
                if map.open(id, spec, context.clone()) {
                    redraw.push(id);
                }
            }
            PopupCommand::Show { id, position } => {
                map.show(id, position);
                redraw.push(id);
            }
            PopupCommand::Move { id, position } => {
                map.move_to(id, position);
                redraw.push(id);
            }
            PopupCommand::Resize { id, size } => {
                map.resize(id, size);
                redraw.push(id);
            }
            PopupCommand::Hide { id } => map.hide(id),
            PopupCommand::Close { id } => map.destroy(id),
            PopupCommand::Redraw { id } => redraw.push(id),
        }
    }
    redraw
}

// -- The real backend -------------------------------------------------------

/// One live popup window: the OS window, its swapchain and the GUI renderer
/// that paints it.
///
/// `surface` is declared before `window` so it drops first (a surface must not
/// outlive the window it presents to).
#[cfg(not(target_arch = "wasm32"))]
pub struct PopupWindow {
    /// The swapchain for this popup.
    pub surface: wgpu::Surface<'static>,
    /// The OS window.
    pub window: Arc<winit::window::Window>,
    /// The surface configuration (size, format, alpha mode).
    pub config: wgpu::SurfaceConfiguration,
    /// Routing key of the window.
    pub key: u64,
    /// The device the surface was configured with (used by a resize).
    pub device: Arc<wgpu::Device>,
    /// The queue the main renderer uses (the GUI renderer resizes with it).
    pub queue: Arc<wgpu::Queue>,
    /// The per-popup GUI renderer, created on the first presented frame.
    pub gui: Option<ferrous_ui_render::GuiRenderer>,
    /// The shared pipeline cache, threaded into the popup's GUI pipelines.
    pub pipeline_cache: Option<Arc<wgpu::PipelineCache>>,
    /// The glyph atlas the popup's GUI renderer was last bound to.
    pub atlas: Option<Arc<ferrous_assets::Font>>,
    /// The per-popup display batch, reused across frames.
    pub batch: ferrous_ui_render::GuiBatch,
    /// When the window was created or last reopened (pool reuse).
    pub opened_at: std::time::Instant,
    /// When the first frame after that open was presented.
    pub first_frame_at: Option<std::time::Instant>,
    /// Presented frames (the idle-popup evidence reads this).
    pub frames: u64,
    /// Copy the next presented frame back to CPU (evidence capture).
    pub capture_next: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl PopupWindow {
    /// Milliseconds from the open to the first presented frame, when it
    /// happened.
    #[must_use]
    pub fn first_frame_ms(&self) -> Option<f64> {
        self.first_frame_at
            .map(|at| at.duration_since(self.opened_at).as_secs_f64() * 1000.0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PopupWindow {
    /// The window's OS handle for attaching it to its owner.
    #[must_use]
    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// The popup's colour format.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Whether the swapchain really blends with what is behind the window.
    #[must_use]
    pub fn transparent(&self) -> bool {
        self.config.alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied
    }

    /// The current physical size.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// The popup's own DPI scale factor.
    ///
    /// A popup can sit on a monitor whose scale differs from the main window's;
    /// its buffer and its text must use this factor, not the main window's.
    #[must_use]
    pub fn scale_factor(&self) -> f64 {
        self.window.scale_factor()
    }

    /// Reconfigures the swapchain at `width x height`.
    pub fn reconfigure(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        if let Some(gui) = &mut self.gui {
            gui.update_output_config(&self.queue, width, height, self.config.format, 1);
        }
    }

    /// Reconfigures unconditionally (after a swapchain that reports
    /// `Lost`/`Outdated`), the way the main window does.
    pub fn reconfigure_current(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// Acquires the next image.
    ///
    /// # Errors
    ///
    /// Returns the wgpu surface error (`Lost`, `Outdated`, `Timeout`, ...).
    pub fn acquire(&mut self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        self.surface.get_current_texture()
    }
}

/// The GPU state a popup surface needs, shared with the main window.
///
/// The main window owns the instance, the adapter and the device, so the
/// desktop backend borrows them for the one call that creates a surface.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct PopupContext<'a> {
    /// The event loop, the only object that can create a window.
    pub event_loop: &'a winit::event_loop::ActiveEventLoop,
    /// The instance the surface is created on.
    pub instance: &'a wgpu::Instance,
    /// The adapter whose capabilities pick the format and alpha mode.
    pub adapter: &'a wgpu::Adapter,
    /// The device the swapchain is configured with.
    pub device: &'a Arc<wgpu::Device>,
    /// The queue the main renderer uses.
    pub queue: &'a Arc<wgpu::Queue>,
    /// The pipeline cache shared with the main renderer.
    pub pipeline_cache: Option<Arc<wgpu::PipelineCache>>,
    /// The main window's format, the fallback when the surface exposes none.
    pub format: wgpu::TextureFormat,
}

/// A popup's `memory-report` label: `popup:{handle}:{part}`.
///
/// The label is the registry identity, so a re-register (a resize, a pooled
/// reopen) replaces the bytes instead of adding a second entry.
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn popup_memory_label(handle: u32, part: &str) -> String {
    format!("popup:{handle}:{part}")
}

/// Bytes of one popup's swapchain estimate (`w * h * 4 * images`).
///
/// The image count is [`POPUP_SWAPCHAIN_IMAGES`] because wgpu exposes none.
#[must_use]
#[cfg(not(target_arch = "wasm32"))]
pub fn popup_swapchain_bytes(width: u32, height: u32) -> u64 {
    u64::from(width.max(1)) * u64::from(height.max(1)) * 4 * u64::from(POPUP_SWAPCHAIN_IMAGES)
}

/// Registers/replaces the swapchain estimate of `handle`.
#[cfg(not(target_arch = "wasm32"))]
fn register_popup_swapchain(handle: u32, config: &wgpu::SurfaceConfiguration) {
    memory::register_gpu(
        popup_memory_label(handle, "swapchain"),
        popup_swapchain_bytes(config.width, config.height),
        Subsystem::Popups,
    );
}

/// Registers/replaces the GUI buffers `gui` owns for `handle`.
#[cfg(not(target_arch = "wasm32"))]
pub fn register_popup_gui(handle: u32, gui: &ferrous_ui_render::GuiRenderer) {
    let (instances, svg) = gui.popup_buffer_bytes();
    memory::register_gpu(
        popup_memory_label(handle, "instances"),
        instances,
        Subsystem::Popups,
    );
    memory::register_gpu(popup_memory_label(handle, "svg"), svg, Subsystem::Popups);
}

/// Drops every `popups` entry a destroyed popup owned.
#[cfg(not(target_arch = "wasm32"))]
fn unregister_popup_memory(handle: u32) {
    memory::unregister_gpu(&popup_memory_label(handle, "instances"), Subsystem::Popups);
    memory::unregister_gpu(&popup_memory_label(handle, "svg"), Subsystem::Popups);
    memory::unregister_gpu(&popup_memory_label(handle, "swapchain"), Subsystem::Popups);
}

/// The winit popup backend.
///
/// It owns one [`PopupWindow`] per handle (the vector index is the handle).
/// Surfaces are created on the same instance, adapter and device as the main
/// window so the glyph atlas binds across both.
///
/// Alpha modes: the surface blends with the desktop (`PreMultiplied`) when the
/// adapter supports it; otherwise the popup is opaque and paints its own
/// background, which is the documented fallback.
#[cfg(not(target_arch = "wasm32"))]
pub struct WinitPopups {
    windows: Vec<Option<PopupWindow>>,
    #[cfg(target_os = "windows")]
    owner: isize,
}

#[cfg(not(target_arch = "wasm32"))]
impl WinitPopups {
    /// A backend with no windows yet: the map can be built in `resumed`, so
    /// routing is live before the renderer exists.
    ///
    /// On Windows the owner is the main window's `HWND`, so every popup is
    /// owned, stays above its owner and never appears in the taskbar.
    #[must_use]
    pub fn new(main: &winit::window::Window) -> Self {
        #[cfg(target_os = "windows")]
        let owner = hwnd_of(main);
        #[cfg(not(target_os = "windows"))]
        let _ = main;
        Self {
            windows: Vec::new(),
            #[cfg(target_os = "windows")]
            owner,
        }
    }

    /// One popup window by handle.
    #[must_use]
    pub fn window(&self, handle: u32) -> Option<&PopupWindow> {
        self.windows.get(handle as usize).and_then(Option::as_ref)
    }

    /// One popup window by handle, mutably.
    pub fn window_mut(&mut self, handle: u32) -> Option<&mut PopupWindow> {
        self.windows
            .get_mut(handle as usize)
            .and_then(Option::as_mut)
    }

    /// Asks the OS for one frame of that window.
    pub fn request_redraw(&self, handle: u32) {
        if let Some(window) = self.window(handle) {
            window.window.request_redraw();
        }
    }

    /// Whether `handle`'s next frame will be copied back.
    pub fn take_capture(&mut self, handle: u32) -> bool {
        match self.window_mut(handle) {
            Some(window) => std::mem::take(&mut window.capture_next),
            None => false,
        }
    }

    /// The live popup windows.
    pub fn iter(&self) -> impl Iterator<Item = &PopupWindow> {
        self.windows.iter().flatten()
    }

    /// The popup's own DPI factor, when it exists.
    #[must_use]
    pub fn window_scale(&self, handle: u32) -> Option<f64> {
        self.window(handle).map(PopupWindow::scale_factor)
    }

    /// Whether `GetForegroundWindow` is the main window or one of our popups.
    ///
    /// A menu that does not take activation keeps the main window foreground;
    /// this is the check that decides whether a `Focused(false)` is a real app
    /// switch (dismiss the overlays) or just a popup interaction (keep them).
    #[cfg(target_os = "windows")]
    #[must_use]
    pub fn foreground_is_ours(&self) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        // SAFETY: `GetForegroundWindow` takes no arguments and returns a
        // borrowed handle that Windows owns.
        let foreground = unsafe { GetForegroundWindow() } as isize;
        if foreground == 0 {
            return false;
        }
        if foreground == self.owner {
            return true;
        }
        self.iter()
            .any(|popup| hwnd_of(&popup.window) == foreground)
    }

    /// A non-Windows host never treats a focus change as its own window.
    #[cfg(not(target_os = "windows"))]
    #[must_use]
    pub fn foreground_is_ours(&self) -> bool {
        false
    }

    fn alpha_mode(caps: &wgpu::SurfaceCapabilities) -> wgpu::CompositeAlphaMode {
        if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Opaque
        }
    }
}

/// The window's `HWND` (needed to make a popup its owned window).
#[cfg(target_os = "windows")]
fn hwnd_of(window: &winit::window::Window) -> isize {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
        _ => 0,
    }
}

/// Adds `WS_EX_NOACTIVATE` so clicking the popup never activates it.
///
/// Without it a normal top-level window takes activation on a click: the main
/// window loses focus (its title bar greys out) and the keyboard moves to the
/// popup, so Escape would no longer reach the menu's owner.
#[cfg(target_os = "windows")]
fn set_no_activate(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    if hwnd == 0 {
        return;
    }
    let window = hwnd as *mut core::ffi::c_void;
    // SAFETY: `hwnd` is a live window created above; the extended style is an
    // `isize` bit set.
    let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
    // SAFETY: the same live window handle; adding one extended style bit.
    unsafe {
        SetWindowLongPtrW(window, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
    }
}

/// Owns its window so it satisfies wgpu's `'static` surface target.
#[cfg(not(target_arch = "wasm32"))]
struct OwnedWindow(Arc<winit::window::Window>);

#[cfg(not(target_arch = "wasm32"))]
impl winit::raw_window_handle::HasWindowHandle for OwnedWindow {
    fn window_handle(
        &self,
    ) -> Result<winit::raw_window_handle::WindowHandle<'_>, winit::raw_window_handle::HandleError>
    {
        self.0.window_handle()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl winit::raw_window_handle::HasDisplayHandle for OwnedWindow {
    fn display_handle(
        &self,
    ) -> Result<winit::raw_window_handle::DisplayHandle<'_>, winit::raw_window_handle::HandleError>
    {
        self.0.display_handle()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PopupBackend for WinitPopups {
    type CreateContext<'a> = PopupContext<'a>;

    fn create(&mut self, spec: PopupSpec, context: PopupContext<'_>) -> Option<u32> {
        use winit::dpi::{PhysicalPosition, PhysicalSize};

        let mut attributes = winit::window::Window::default_attributes()
            .with_decorations(false)
            .with_visible(false)
            .with_transparent(spec.transparent)
            .with_resizable(spec.resizable)
            .with_active(false)
            .with_inner_size(PhysicalSize::new(spec.size.0.max(1), spec.size.1.max(1)))
            .with_position(PhysicalPosition::new(spec.position.0, spec.position.1));
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes
                .with_owner_window(self.owner)
                .with_skip_taskbar(true);
        }
        let window = Arc::new(context.event_loop.create_window(attributes).ok()?);
        // A menu must never take activation: the main window keeps the keyboard
        // (Escape still reaches it) and its title bar never greys out.
        #[cfg(target_os = "windows")]
        set_no_activate(hwnd_of(&window));
        let surface = context
            .instance
            .create_surface(OwnedWindow(window.clone()))
            .ok()?;
        let caps = surface.get_capabilities(context.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(context.format);
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if crate::screenshot::supports_copy_src(&caps) {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let config = wgpu::SurfaceConfiguration {
            usage,
            format,
            width: spec.size.0.max(1),
            height: spec.size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: Self::alpha_mode(&caps),
            view_formats: vec![],
            // A popup is a small surface shown right after input: the shortest
            // queue (one frame in flight) keeps input-to-present low.
            desired_maximum_frame_latency: 1,
        };
        surface.configure(context.device, &config);
        let key = key_of(window.id());
        let handle = u32::try_from(self.windows.len()).ok()?;
        self.windows.push(Some(PopupWindow {
            surface,
            window,
            config,
            key,
            device: context.device.clone(),
            queue: context.queue.clone(),
            pipeline_cache: context.pipeline_cache,
            atlas: None,
            gui: None,
            batch: ferrous_ui_render::GuiBatch::new(),
            opened_at: std::time::Instant::now(),
            first_frame_at: None,
            frames: 0,
            capture_next: false,
        }));
        // The swapchain exists from creation; the GUI buffers are registered
        // when the first frame lazily creates the popup's `GuiRenderer`.
        if let Some(window) = self.window(handle) {
            register_popup_swapchain(handle, &window.config);
        }
        Some(handle)
    }

    fn set_visible(&mut self, handle: u32, visible: bool) {
        if let Some(window) = self.window(handle) {
            window.window.set_visible(visible);
        }
    }

    fn set_position(&mut self, handle: u32, position: (i32, i32)) {
        if let Some(window) = self.window(handle) {
            window
                .window
                .set_outer_position(winit::dpi::PhysicalPosition::new(position.0, position.1));
        }
    }

    fn set_size(&mut self, handle: u32, size: (u32, u32)) {
        if let Some(window) = self.window_mut(handle) {
            let _ = window
                .window
                .request_inner_size(winit::dpi::PhysicalSize::new(size.0.max(1), size.1.max(1)));
            window.reconfigure(size.0, size.1);
            register_popup_swapchain(handle, &window.config);
        }
    }

    fn destroy(&mut self, handle: u32) {
        if let Some(slot) = self.windows.get_mut(handle as usize) {
            // Dropping the surface first, then the window, closes the popup.
            slot.take();
            unregister_popup_memory(handle);
        }
    }

    fn transparent(&self, handle: u32) -> bool {
        self.window(handle).is_some_and(PopupWindow::transparent)
    }

    fn key(&self, handle: u32) -> Option<u64> {
        self.window(handle).map(|window| window.key)
    }

    fn reopened(&mut self, handle: u32) {
        if let Some(window) = self.window_mut(handle) {
            window.opened_at = std::time::Instant::now();
            window.first_frame_at = None;
        }
    }

    fn arm_capture(&mut self, handle: u32) {
        if let Some(window) = self.window_mut(handle) {
            window.capture_next = true;
        }
    }
}

/// A headless backend: no OS window, deterministic handles and routing keys.
///
/// Shared by the window-map and runner-popup unit tests.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct MockBackend {
    windows: std::collections::HashMap<u32, (PopupSpec, bool, bool)>,
    next: u32,
    /// How many OS windows the pool has actually created.
    pub(crate) created: usize,
}

#[cfg(test)]
impl MockBackend {
    /// The number of live (non-destroyed) windows.
    #[allow(dead_code)] // used by the runner popup tests when they check the pool
    pub(crate) fn live(&self) -> usize {
        self.windows.len()
    }

    /// Whether the mock window is OS-visible.
    #[allow(dead_code)] // used by the occlusion test
    pub(crate) fn visible(&self, handle: u32) -> bool {
        self.windows
            .get(&handle)
            .is_some_and(|(_, visible, _)| *visible)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        drain, key_of, MockBackend, PopupBackend, PopupCommand, PopupId, PopupQueue, PopupSpec,
        WindowMap, WindowTarget,
    };
    use winit::window::WindowId;

    impl PopupBackend for MockBackend {
        type CreateContext<'a> = ();

        fn create(&mut self, spec: PopupSpec, (): ()) -> Option<u32> {
            let handle = self.next;
            self.next += 1;
            self.created += 1;
            self.windows.insert(handle, (spec, false, spec.transparent));
            Some(handle)
        }
        fn set_visible(&mut self, handle: u32, visible: bool) {
            if let Some(window) = self.windows.get_mut(&handle) {
                window.1 = visible;
            }
        }
        fn set_position(&mut self, handle: u32, position: (i32, i32)) {
            if let Some(window) = self.windows.get_mut(&handle) {
                window.0.position = position;
            }
        }
        fn set_size(&mut self, handle: u32, size: (u32, u32)) {
            if let Some(window) = self.windows.get_mut(&handle) {
                window.0.size = size;
            }
        }
        fn destroy(&mut self, handle: u32) {
            self.windows.remove(&handle);
        }
        fn transparent(&self, handle: u32) -> bool {
            self.windows
                .get(&handle)
                .is_some_and(|(_, _, transparent)| *transparent)
        }
        fn key(&self, handle: u32) -> Option<u64> {
            self.windows
                .contains_key(&handle)
                .then_some(u64::from(handle) + 1000)
        }
        fn key_of(&self, window: WindowId) -> u64 {
            // The mock derives its routing key from the window's numeric id,
            // so a test can address a popup with `WindowId::from`.
            u64::from(window) + 1000
        }
    }

    fn map() -> WindowMap<MockBackend> {
        WindowMap::new(WindowId::from(7), MockBackend::default())
    }

    #[test]
    fn the_queue_reserves_ids_at_once_and_drains_in_order() {
        let mut queue = PopupQueue::new();
        assert!(queue.is_empty());
        let first = queue.open(PopupSpec::default());
        let second = queue.open(PopupSpec::default());
        assert_eq!(first.0 + 1, second.0, "ids are reserved synchronously");
        queue.show(first, (10, 20));
        queue.move_to(first, (30, 40));
        queue.resize(first, (320, 180));
        queue.hide(second);
        queue.close(first);
        queue.redraw(second);
        let commands = queue.drain();
        assert_eq!(commands.len(), 8);
        assert!(matches!(commands[0], PopupCommand::Open { id, .. } if id == first));
        assert!(matches!(commands[1], PopupCommand::Open { id, .. } if id == second));
        assert!(
            matches!(commands[2], PopupCommand::Show { id, position } if id == first && position == (10, 20))
        );
        assert!(
            matches!(commands[3], PopupCommand::Move { id, position } if id == first && position == (30, 40))
        );
        assert!(
            matches!(commands[4], PopupCommand::Resize { id, size } if id == first && size == (320, 180))
        );
        assert!(matches!(commands[5], PopupCommand::Hide { id } if id == second));
        assert!(matches!(commands[6], PopupCommand::Close { id } if id == first));
        assert!(matches!(commands[7], PopupCommand::Redraw { id } if id == second));
        assert!(queue.is_empty(), "drain empties the queue");
    }

    #[test]
    fn draining_keeps_the_queue_ids_and_requests_a_frame() {
        let mut map = map();
        let mut queue = PopupQueue::new();
        let id = queue.open(PopupSpec {
            position: (40, 60),
            size: (300, 200),
            transparent: true,
            resizable: false,
        });
        queue.show(id, (44, 66));
        let redraw = drain(&mut map, &mut queue, ());
        assert_eq!(redraw, vec![id, id], "open and show each ask for a frame");
        assert!(map.contains(id), "the queue id is the map id");
        assert!(map.visible(id));
        assert_eq!(map.spec(id).expect("spec").position, (44, 66));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn a_pooled_popup_keeps_its_os_window_for_a_new_id() {
        let mut map = map();
        let mut queue = PopupQueue::new();
        let first = queue.open(PopupSpec::default());
        drain(&mut map, &mut queue, ());
        let first_handle = map.handle(first).expect("handle");
        queue.hide(first);
        drain(&mut map, &mut queue, ());
        let second = queue.open(PopupSpec {
            position: (10, 20),
            size: (300, 200),
            transparent: true,
            resizable: false,
        });
        drain(&mut map, &mut queue, ());
        assert_ne!(first, second, "every open reserves a new id");
        assert_eq!(
            map.handle(second),
            Some(first_handle),
            "the hidden OS window is reused"
        );
        assert_eq!(map.len(), 1, "no second OS window");
        assert!(!map.visible(second));
        assert_eq!(map.backend().created, 1);
    }

    #[test]
    fn a_popup_window_id_routes_to_its_popup() {
        let mut map = map();
        let mut queue = PopupQueue::new();
        let id = queue.open(PopupSpec::default());
        drain(&mut map, &mut queue, ());
        let window = WindowId::from(0);
        assert_eq!(map.route(window), WindowTarget::Popup(id));
        assert_eq!(map.route(WindowId::from(7)), WindowTarget::Main);
        assert_eq!(key_of(window), key_of(window), "keys are stable");
    }

    #[test]
    fn create_show_move_resize_hide_destroy() {
        let mut window_map = map();
        let id = window_map.create(PopupSpec::default(), ()).expect("create");
        assert!(window_map.contains(id));
        assert!(!window_map.visible(id));

        window_map.show(id, (30, 40));
        assert!(window_map.visible(id));
        assert_eq!(window_map.spec(id).expect("spec").position, (30, 40));

        window_map.move_to(id, (50, 60));
        window_map.resize(id, (320, 180));
        let spec = window_map.spec(id).expect("spec");
        assert_eq!(spec.position, (50, 60));
        assert_eq!(spec.size, (320, 180));

        window_map.hide(id);
        assert!(!window_map.visible(id));
        window_map.destroy(id);
        assert!(!window_map.contains(id));
        assert!(window_map.is_empty());
    }

    #[test]
    fn a_hidden_popup_is_pooled_and_reused() {
        let mut window_map = map();
        let first = window_map.create(PopupSpec::default(), ()).expect("create");
        let first_handle = window_map.handle(first).expect("handle");
        window_map.hide(first);
        let second = window_map
            .create(
                PopupSpec {
                    position: (10, 20),
                    size: (300, 200),
                    transparent: true,
                    resizable: false,
                },
                (),
            )
            .expect("reuse");
        assert_eq!(
            window_map.handle(second),
            Some(first_handle),
            "the OS window is reused"
        );
        let spec = window_map.spec(second).expect("spec");
        assert_eq!(spec.size, (300, 200));
        assert!(!window_map.visible(second));
        assert_eq!(window_map.len(), 1, "no second OS window");
        assert_eq!(window_map.backend().created, 1);
    }

    #[test]
    fn destroying_one_popup_keeps_the_others_addressable() {
        let mut window_map = map();
        let first = window_map.create(PopupSpec::default(), ()).expect("first");
        window_map.show(first, (0, 0));
        let second = window_map.create(PopupSpec::default(), ()).expect("second");
        assert_ne!(first, second);
        window_map.destroy(first);
        assert!(!window_map.contains(first));
        assert!(window_map.contains(second));
        window_map.show(second, (1, 2));
        assert!(window_map.visible(second));
        assert_eq!(window_map.spec(second).expect("spec").position, (1, 2));
    }

    #[test]
    fn the_main_window_routes_to_main() {
        let window_map = map();
        assert_eq!(
            window_map.route_key(window_map.main_key()),
            WindowTarget::Main
        );
        assert_eq!(window_map.route_key(u64::MAX - 1), WindowTarget::Main);
    }

    #[test]
    fn the_pool_keeps_at_most_two_hidden_windows() {
        let mut window_map = map();
        let ids: Vec<PopupId> = (0..5)
            .map(|_| window_map.create(PopupSpec::default(), ()).expect("create"))
            .collect();
        assert_eq!(window_map.len(), 5, "five visible popups");
        for id in ids {
            window_map.hide(id);
        }
        assert_eq!(
            window_map.len(),
            super::POPUP_POOL_CAP,
            "hiding beyond the cap destroys the oldest hidden window"
        );
        assert_eq!(window_map.backend().live(), super::POPUP_POOL_CAP);
    }

    #[test]
    fn occluding_hides_and_restoring_re_shows_the_visible_popups() {
        let mut window_map = map();
        let shown = window_map.create(PopupSpec::default(), ()).expect("shown");
        window_map.show(shown, (10, 20));
        let hidden = window_map.create(PopupSpec::default(), ()).expect("hidden");
        window_map.set_occluded(true);
        assert!(window_map.visible(shown), "logical visibility is kept");
        assert!(
            !window_map
                .backend()
                .visible(window_map.handle(shown).expect("handle")),
            "the OS window is hidden"
        );
        window_map.set_occluded(false);
        assert!(
            window_map
                .backend()
                .visible(window_map.handle(shown).expect("handle")),
            "restore re-shows it"
        );
        assert!(
            !window_map
                .backend()
                .visible(window_map.handle(hidden).expect("handle")),
            "a closed popup stays hidden"
        );
    }

    #[test]
    fn a_window_move_shifts_only_the_visible_popups() {
        let mut window_map = map();
        let shown = window_map.create(PopupSpec::default(), ()).expect("shown");
        window_map.show(shown, (10, 20));
        let hidden = window_map.create(PopupSpec::default(), ()).expect("hidden");
        window_map.shift_visible(5, -7);
        assert_eq!(window_map.spec(shown).expect("spec").position, (15, 13));
        assert_eq!(window_map.spec(hidden).expect("spec").position, (0, 0));
    }

    #[test]
    fn a_popup_routes_by_its_stable_key() {
        let mut window_map = map();
        let id = window_map.create(PopupSpec::default(), ()).expect("create");
        // The mock assigns key handle + 1000; handle 0 is the first popup.
        assert_eq!(window_map.route_key(1000), WindowTarget::Popup(id));
    }

    #[test]
    fn a_transparent_popup_reports_the_backend_answer() {
        let mut window_map = map();
        let id = window_map.create(PopupSpec::default(), ()).expect("create");
        assert!(window_map.transparent(id));
        assert_eq!(PopupId(0), id);
    }
}
