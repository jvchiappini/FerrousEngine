//! `Runner` struct definition and constructor.

use ferrous_assets::{AssetHandle, AssetServer, Font};
use ferrous_core::{
    AnimationSystem, BehaviorSystem, InputState, TimeClock, TimeSystem,
    TransformSystem, VelocitySystem, Viewport, World,
};
use ferrous_ecs::prelude::{ResourceMap, Stage, StagedScheduler};
use ferrous_gui::Ui;
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
use crate::graphics::GraphicsState;
use crate::traits::FerrousApp;

// ─── Runner ─────────────────────────────────────────────────────────────────

pub(crate) struct Runner<A: FerrousApp> {
    pub(super) app: A,
    pub(super) config: AppConfig,
    pub(super) window: Option<Arc<Window>>,
    pub(super) graphics: Option<GraphicsState>,
    pub(super) ui: Ui,
    pub(super) input: InputState,
    pub(super) window_size: (u32, u32),
    pub(super) viewport: Viewport,
    pub(super) clock: TimeClock,
    pub(super) world: World,
    pub(super) font: Option<Font>,
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
}

impl<A: FerrousApp> Runner<A> {
    pub(super) fn new(app: A, config: AppConfig) -> Self {
        let mut systems = StagedScheduler::new();
        systems.add(Stage::PreUpdate, TimeSystem);
        systems.add(Stage::Update, VelocitySystem);
        systems.add(Stage::Update, AnimationSystem);
        systems.add(Stage::Update, BehaviorSystem);
        systems.add(Stage::PostUpdate, TransformSystem);

        Self {
            app,
            config,
            window: None,
            graphics: None,
            ui: Ui::new(),
            input: InputState::new(),
            viewport: Viewport { x: 0, y: 0, width: 0, height: 0 },
            window_size: (0, 0),
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
        }
    }
}
