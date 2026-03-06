//! Typed event resources and parameter helpers.
//!
//! Events are stored as a normal resource (`ResourceMap::insert`), and
//! systems receive them via the `EventWriter<T>` or `EventReader<T>` params.
//! Both writer and reader implement [`crate::system_param::SystemParam`], so
//! they can be injected automatically into plain-function systems.
//!
//! # Usage
//!
//! ```rust,ignore
//! resources.insert(Events::<CollisionEvent>::new());
//!
//! fn detect(
//!     mut events: EventWriter<CollisionEvent>,
//!     query: Query<(Entity, &Transform, &Collider)>,
//! ) {
//!     // ...
//!     events.send(CollisionEvent { a, b });
//! }
//!
//! fn react(
//!     mut events: EventReader<CollisionEvent>,
//!     mut health: Query<&mut Health>,
//! ) {
//!     for ev in events.read() {
//!         // ...
//!     }
//! }
//! ```

use std::any::TypeId;

use crate::resource::ResourceMap;
use crate::system_param::SystemParam;
use crate::world::World;

/// Storage for a stream of events of type `T`.
///
/// `current` collects events produced during the current frame.  Calling
/// [`Events::update`] rotates the buffers, moving `current` into `previous`
/// so that readers can consume them and writers can start fresh.
#[derive(Debug, Default)]
pub struct Events<T: Send + Sync + 'static> {
    current: Vec<T>,
    previous: Vec<T>,
}

impl<T: Send + Sync + 'static> Events<T> {
    /// Create an empty event stream.
    pub fn new() -> Self {
        Self {
            current: Vec::new(),
            previous: Vec::new(),
        }
    }

    /// Send an event into the stream.  This appends to the `current` buffer.
    pub fn send(&mut self, event: T) {
        self.current.push(event);
    }

    /// Read and consume all events produced in the *previous* frame.
    ///
    /// The returned iterator borrows from the previous buffer; the reader's
    /// cursor is advanced to the end so subsequent calls start later.
    pub fn read<'w>(&'w mut self) -> impl Iterator<Item = &'w T> {
        let slice = &self.previous;
        slice.iter()
    }

    /// Advance the frame: drop the old `previous` buffer and swap in
    /// whatever has been collected in `current`.
    pub fn update(&mut self) {
        self.previous.clear();
        std::mem::swap(&mut self.previous, &mut self.current);
    }
}

/// Writable handle injected into systems.
pub struct EventWriter<'w, T: Send + Sync + 'static> {
    events: &'w mut Events<T>,
}

impl<'w, T: Send + Sync + 'static> EventWriter<'w, T> {
    /// Send an event.  Equivalent to calling [`Events::send`] on the underlying
    /// resource.
    #[inline]
    pub fn send(&mut self, event: T) {
        self.events.send(event);
    }
}

/// Read-only handle injected into systems.
pub struct EventReader<'w, T: Send + Sync + 'static> {
    events: &'w Events<T>,
    cursor: usize,
}

impl<'w, T: Send + Sync + 'static> EventReader<'w, T> {
    /// Iterate over the events produced in the previous frame, starting at the
    /// reader's current cursor.  The cursor is advanced to the end after
    /// iteration.
    #[inline]
    pub fn read(&mut self) -> impl Iterator<Item = &'w T> {
        let slice = &self.events.previous;
        let start = self.cursor;
        self.cursor = slice.len();
        slice[start..].iter()
    }
}

// ---------------------------------------------------------------------------
// SystemParam implementations

unsafe impl<'a, T: Send + Sync + 'static> SystemParam for EventWriter<'a, T> {
    type State = ();
    type Item<'w> = EventWriter<'w, T>;

    fn init(_world: &World, _resources: &ResourceMap) {
        // nothing to cache; we assume the caller inserted the resource
        // beforehand.  `fetch` will panic if it is missing.
    }

    #[inline]
    unsafe fn fetch<'w>(
        _state: &'w mut (),
        _world: &'w World,
        resources: &'w ResourceMap,
    ) -> EventWriter<'w, T> {
        let ptr = resources
            .get_mut_ptr::<Events<T>>()
            .expect("EventWriter: Events resource missing");
        EventWriter { events: &mut *ptr }
    }

    fn res_writes() -> Vec<TypeId> {
        vec![TypeId::of::<Events<T>>()]
    }
}

unsafe impl<'a, T: Send + Sync + 'static> SystemParam for EventReader<'a, T> {
    type State = usize; // cursor position
    type Item<'w> = EventReader<'w, T>;

    fn init(_world: &World, _resources: &ResourceMap) -> usize {
        0
    }

    #[inline]
    unsafe fn fetch<'w>(
        state: &'w mut usize,
        _world: &'w World,
        resources: &'w ResourceMap,
    ) -> EventReader<'w, T> {
        let events = resources
            .get::<Events<T>>()
            .expect("EventReader: Events resource missing");
        EventReader {
            events,
            cursor: *state,
        }
    }

    fn res_reads() -> Vec<TypeId> {
        vec![TypeId::of::<Events<T>>()]
    }
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_param::SystemParam;
    use crate::world::World;

    #[test]
    fn writer_can_send() {
        let mut resources = ResourceMap::new();
        resources.insert(Events::<u32>::new());
        let world = World::new();
        let mut state = EventWriter::<u32>::init(&world, &resources);
        {
            let mut w = unsafe { EventWriter::<u32>::fetch(&mut state, &world, &resources) };
            w.send(42);
        }
        assert_eq!(resources.get::<Events<u32>>().unwrap().current.len(), 1);
    }

    #[test]
    fn reader_reads_previous_frame() {
        let mut events = Events::new();
        events.send(7);
        events.update();
        let mut resources = ResourceMap::new();
        resources.insert(events);

        let world = World::new();
        let mut state = EventReader::<u32>::init(&world, &resources);
        let mut r = unsafe { EventReader::<u32>::fetch(&mut state, &world, &resources) };
        let collected: Vec<_> = r.read().cloned().collect();
        assert_eq!(collected, vec![7]);
    }
}
