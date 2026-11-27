//! `Runner` struct definition and constructor.

use ferrous_assets::{AssetHandle, AssetServer, Font};
use ferrous_core::{
    AnimationSystem, BehaviorSystem, InputState, TimeClock, TimeSystem, TransformSystem,
    VelocitySystem, Viewport, World,
};
use ferrous_ecs::prelude::{ResourceMap, Stage, StagedScheduler};
use std::sync::Arc;
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use ferrous_assets::font_importer::FontData;

use crate::builder::AppConfig;
use crate::context::AppContext;
use crate::graphics::GraphicsState;
use crate::redraw::{Scheduler, Waker};
use crate::render_context::RenderContext;
use crate::traits::{FerrousApp, FileDropEvent};

// ─── Runner ─────────────────────────────────────────────────────────────────

pub(crate) struct Runner<A: FerrousApp + 'static> {
    pub(super) app: A,
    pub(super) config: AppConfig,
    /// The GPU state. Declared before `window` so it is dropped first: it holds
    /// a surface created from that window.
    pub(super) graphics: Option<GraphicsState>,
    pub(super) window: Option<Arc<Window>>,
    pub(super) ui: ferrous_gui::UiSystem<A>,
    pub(super) input: InputState,
    pub(super) window_size: (u32, u32),
    /// The window's current DPI scale factor.
    pub(super) scale_factor: f64,
    /// The main window plus owned popups (popup-capable hosts only).
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) windows: Option<crate::windows::WindowMap<crate::windows::WinitPopups>>,
    /// The composite UI atlas, extendable when a frame needs a fallback glyph.
    pub(super) runtime_font: Option<crate::runtime_font::RuntimeFont>,
    /// Characters the last frame's text asked for and the atlas lacks.
    pub(super) glyph_requests: Vec<char>,
    /// Popup commands queued by the app.
    pub(super) popups: crate::windows::PopupQueue,
    pub(super) viewport: Viewport,
    pub(super) clock: TimeClock,
    pub(super) world: World,
    pub(super) font: Option<std::sync::Arc<Font>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) font_asset_handle: Option<AssetHandle<FontData>>,
    #[cfg(target_arch = "wasm32")]
    pub(super) gfx_pending: Option<Rc<RefCell<Option<GraphicsState>>>>,
    #[cfg(target_arch = "wasm32")]
    pub(super) font_pending: Option<Rc<RefCell<Option<Font>>>>,
    pub(super) last_frame: Instant,
    pub(super) next_frame_deadline: Option<Instant>,
    pub(super) last_action_time: Instant,
    pub(super) systems: StagedScheduler,
    pub(super) resources: ResourceMap,
    pub(super) asset_server: AssetServer,
    /// Whether a Ctrl modifier key is currently held (used for keyboard shortcuts like Ctrl+A).
    pub(super) ctrl_held: bool,
    /// Whether a Shift modifier key is currently held (used for selection shortcuts).
    pub(super) shift_held: bool,
    /// Tracks the last deterministic time absolute value passed to render frames
    pub(super) last_deterministic_t: Option<f64>,
    /// File-drag events waiting for the next frame's `AppContext`.
    pub(super) pending_file_drops: Vec<FileDropEvent>,
    /// Whether the close path (hide + exit) has already run.
    pub(super) closing: bool,
    /// The main window's last reported outer position, so a `Moved` can shift
    /// the open popups by the same delta in the same callback.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) last_window_position: Option<(i32, i32)>,
    /// GPU and UI-font probes started before the window existed (prompt 17 S3).
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) startup_work: Option<crate::graphics::StartupWork>,
    /// Window show policy for the first frame (prompt 17 S3b).
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) startup: Option<super::show::StartupGate>,
    /// The GPU/first-frame initialisation running off the event loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) init: Option<super::show::InitJob>,
    /// Continuous vs on-demand frame scheduling (prompt 18 P1).
    pub(super) redraw: Scheduler,
    /// The next `RedrawRequested` may be the one the OS sends when the hidden
    /// window is first shown; the first frame is already on the swapchain.
    /// Only swallowed when nothing asked for a frame in the meantime.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) skip_show_redraw: bool,
    /// The scheduler's request count when the window was shown. A later
    /// `RedrawRequested` that finds the count unchanged is the redundant OS
    /// show paint; a moved count is a real frame (prompt 15 Phase 9D finding 1).
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) show_request_mark: u64,
    /// Wakes the loop from background workers.
    pub(super) waker: Waker,
    /// Opt-in persistent off-screen UI present path (prompt 18 P7).
    pub(super) ui_presenter: Option<ferrous_ui_render::UiPresenter>,
    /// Monotonic main-frame counter. A popup renders its overlay batch once per
    /// generation; a readback worker's wake at the same generation only flushes
    /// the mapped frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) main_frame_generation: u64,
}

impl<A: FerrousApp + 'static> Runner<A> {
    pub(super) fn new(
        app: A,
        config: AppConfig,
        proxy: winit::event_loop::EventLoopProxy<crate::redraw::EngineEvent>,
    ) -> Self {
        let mut systems = StagedScheduler::new();
        systems.add(Stage::PreUpdate, TimeSystem);
        systems.add(Stage::Update, VelocitySystem);
        systems.add(Stage::Update, AnimationSystem);
        systems.add(Stage::Update, BehaviorSystem);
        systems.add(Stage::PostUpdate, TransformSystem);

        #[cfg(not(target_arch = "wasm32"))]
        let startup_work = Some(crate::graphics::StartupWork::start(&config));

        let redraw_policy = config.redraw_policy;

        Self {
            app,
            config,
            window: None,
            graphics: None,
            ui: ferrous_gui::UiSystem::new(),
            input: InputState::new(),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            window_size: (0, 0),
            scale_factor: 1.0,
            #[cfg(not(target_arch = "wasm32"))]
            windows: None,
            runtime_font: None,
            glyph_requests: Vec::new(),
            popups: crate::windows::PopupQueue::new(),
            clock: TimeClock::new(),
            world: World::new(),
            font: None,
            #[cfg(not(target_arch = "wasm32"))]
            font_asset_handle: None,
            #[cfg(target_arch = "wasm32")]
            gfx_pending: None,
            #[cfg(target_arch = "wasm32")]
            font_pending: None,
            last_frame: Instant::now(),
            next_frame_deadline: None,
            last_action_time: Instant::now(),
            systems,
            resources: ResourceMap::new(),
            asset_server: AssetServer::new(),
            ctrl_held: false,
            shift_held: false,
            last_deterministic_t: None,
            pending_file_drops: Vec::new(),
            closing: false,
            #[cfg(not(target_arch = "wasm32"))]
            last_window_position: None,
            #[cfg(not(target_arch = "wasm32"))]
            startup_work,
            #[cfg(not(target_arch = "wasm32"))]
            startup: None,
            #[cfg(not(target_arch = "wasm32"))]
            init: None,
            redraw: Scheduler::new(redraw_policy),
            #[cfg(not(target_arch = "wasm32"))]
            skip_show_redraw: false,
            #[cfg(not(target_arch = "wasm32"))]
            show_request_mark: 0,
            waker: Waker::from_proxy(proxy),
            ui_presenter: None,
            #[cfg(not(target_arch = "wasm32"))]
            main_frame_generation: 0,
        }
    }

    /// Builds the one `AppContext` every callback shares.
    ///
    /// Every call site used to spell out the ~25-field literal by hand; one
    /// builder keeps `AppContext` and `Runner` in sync when either grows.
    /// Returns `None` before the window or the GPU state exists.
    #[allow(clippy::too_many_arguments)] // mirrors the `AppContext` fields
    pub(super) fn with_ctx<R>(
        &mut self,
        window: &Arc<Window>,
        time: ferrous_core::Time,
        render_stats: ferrous_core::RenderStats,
        camera_eye: ferrous_core::glam::Vec3,
        camera_target: ferrous_core::glam::Vec3,
        window_size: (u32, u32),
        scale_factor: f64,
        callback: impl FnOnce(&mut A, &mut AppContext<'_>) -> R,
    ) -> Option<R> {
        let Runner {
            app,
            graphics,
            input,
            font,
            glyph_requests,
            popups,
            world,
            viewport,
            asset_server,
            waker,
            ..
        } = self;
        let gfx = graphics.as_mut()?;
        let backend = gfx.renderer.context.backend;
        let waker = waker.clone();
        let mut ctx = AppContext {
            input,
            font: font.as_deref(),
            glyph_requests,
            popups,
            time,
            window_size,
            scale_factor,
            window,
            viewport: *viewport,
            render_stats,
            camera_eye,
            camera_target,
            gizmos: Vec::new(),
            world,
            render: RenderContext::new(&mut gfx.renderer),
            asset_server,
            exit_requested: false,
            requested_resolution_scale: None,
            keep_awake_requested: false,
            screenshot_requested: false,
            redraw_requested: false,
            redraw_at: None,
            waker: &waker,
            _gpu_backend: backend,
        };
        Some(callback(app, &mut ctx))
    }
}
