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
//! All coordinates are physical pixels; the caller derives them from the
//! monitor work area and the `GUIMaker` placement solver.

use std::hash::{DefaultHasher, Hash, Hasher};

use winit::window::WindowId;

/// A popup's stable id, assigned by the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PopupId(pub u32);

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

/// The OS window behind the map.
///
/// `handle` is opaque to the map. A backend that cannot create windows (a
/// headless unit test, or a host without popup support) returns `None` and the
/// map degrades to "no popups".
pub trait PopupBackend {
    /// Creates a hidden window and returns its handle.
    fn create(&mut self, spec: PopupSpec) -> Option<u32>;
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
    /// The routing key for an event's `WindowId`.
    fn key_of(&self, window: WindowId) -> u64 {
        key_of(window)
    }
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
    key: u64,
}

/// The main window plus every popup, live or pooled.
pub struct WindowMap {
    main_key: u64,
    backend: Box<dyn PopupBackend>,
    /// Slot table: ids are indices and never move, so destroying one never
    /// renumbers the others.
    entries: Vec<Option<Entry>>,
}

impl WindowMap {
    /// Builds a map over the main window's routing key.
    #[must_use]
    pub fn new(main: WindowId, backend: Box<dyn PopupBackend>) -> Self {
        Self {
            main_key: key_of(main),
            backend,
            entries: Vec::new(),
        }
    }

    /// The main window's routing key.
    #[must_use]
    pub fn main_key(&self) -> u64 {
        self.main_key
    }

    /// Creates a popup, reusing a hidden one when possible.
    ///
    /// The popup starts hidden. Returns `None` when the backend cannot create
    /// a window; the caller then keeps the overlay in the main window.
    pub fn create(&mut self, spec: PopupSpec) -> Option<PopupId> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|entry| !entry.visible))
        {
            let handle = {
                let entry = self.entries[index].as_mut()?;
                entry.spec = spec;
                entry.visible = false;
                entry.handle
            };
            self.backend.set_position(handle, spec.position);
            self.backend.set_size(handle, spec.size);
            self.backend.set_visible(handle, false);
            return Some(PopupId(index as u32));
        }
        let handle = self.backend.create(spec)?;
        let key = self.backend.key(handle)?;
        let id = PopupId(self.entries.len() as u32);
        self.entries.push(Some(Entry {
            handle,
            spec,
            visible: false,
            key,
        }));
        Some(id)
    }

    /// Shows `id` at `position`.
    pub fn show(&mut self, id: PopupId, position: (i32, i32)) {
        let Some(handle) = self.entry_mut(id).map(|entry| {
            entry.spec.position = position;
            entry.visible = true;
            entry.handle
        }) else {
            return;
        };
        self.backend.set_position(handle, position);
        self.backend.set_visible(handle, true);
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
    pub fn hide(&mut self, id: PopupId) {
        let Some(handle) = self.entry_mut(id).map(|entry| {
            entry.visible = false;
            entry.handle
        }) else {
            return;
        };
        self.backend.set_visible(handle, false);
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

    /// Where an event for `window` goes.
    #[must_use]
    pub fn route(&self, window: WindowId) -> WindowTarget {
        self.route_key(self.backend.key_of(window))
    }

    /// [`Self::route`] by routing key (the pure half, used by tests).
    #[must_use]
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

    fn entry(&self, id: PopupId) -> Option<&Entry> {
        self.entries.get(id.0 as usize).and_then(Option::as_ref)
    }

    fn entry_mut(&mut self, id: PopupId) -> Option<&mut Entry> {
        self.entries.get_mut(id.0 as usize).and_then(Option::as_mut)
    }
}

#[cfg(test)]
mod tests {
    use super::{PopupBackend, PopupId, PopupSpec, WindowMap, WindowTarget};
    use std::collections::HashMap;
    use winit::window::WindowId;

    #[derive(Default)]
    struct MockBackend {
        windows: HashMap<u32, (PopupSpec, bool, bool)>,
        next: u32,
    }

    impl PopupBackend for MockBackend {
        fn create(&mut self, spec: PopupSpec) -> Option<u32> {
            let handle = self.next;
            self.next += 1;
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
        fn key_of(&self, _window: WindowId) -> u64 {
            u64::MAX
        }
    }

    const MAIN_KEY: u64 = 7;

    fn map() -> WindowMap {
        // The map only needs the main window's key; the backend never sees it.
        WindowMap {
            main_key: MAIN_KEY,
            backend: Box::new(MockBackend::default()),
            entries: Vec::new(),
        }
    }

    #[test]
    fn create_show_move_resize_hide_destroy() {
        let mut window_map = map();
        let id = window_map.create(PopupSpec::default()).expect("create");
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
        let first = window_map.create(PopupSpec::default()).expect("create");
        window_map.hide(first);
        let second = window_map
            .create(PopupSpec {
                position: (10, 20),
                size: (300, 200),
                transparent: true,
                resizable: false,
            })
            .expect("reuse");
        assert_eq!(first, second, "the pooled popup is reused");
        let spec = window_map.spec(second).expect("spec");
        assert_eq!(spec.size, (300, 200));
        assert!(!window_map.visible(second));
        assert_eq!(window_map.len(), 1, "no second OS window");
    }

    #[test]
    fn destroying_one_popup_keeps_the_others_addressable() {
        let mut window_map = map();
        let first = window_map.create(PopupSpec::default()).expect("first");
        window_map.show(first, (0, 0));
        let second = window_map.create(PopupSpec::default()).expect("second");
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
        assert_eq!(window_map.route_key(MAIN_KEY), WindowTarget::Main);
        assert_eq!(window_map.route_key(u64::MAX - 1), WindowTarget::Main);
    }

    #[test]
    fn a_popup_routes_by_its_stable_key() {
        let mut window_map = map();
        let id = window_map.create(PopupSpec::default()).expect("create");
        // The mock assigns key handle + 1000; handle 0 is the first popup.
        assert_eq!(window_map.route_key(1000), WindowTarget::Popup(id));
    }

    #[test]
    fn a_transparent_popup_reports_the_backend_answer() {
        let mut window_map = map();
        let id = window_map.create(PopupSpec::default()).expect("create");
        assert!(window_map.transparent(id));
        assert_eq!(PopupId(0), id);
    }
}
