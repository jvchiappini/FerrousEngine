use alloc::vec::Vec;
use core::cell::RefCell;
use alloc::rc::Rc;

/// A subscriber that can be notified. For UI it's usually `NodeId`.
pub trait Observer: Clone + PartialEq {}
impl<T: Clone + PartialEq> Observer for T {}

/// Core Trait for 100% Modularity.
/// A game engine using ECS can implement this trait on its Component queries,
/// completely avoiding the overhead of duplicating state into the UI Tree.
pub trait Observable<T, S: Observer> {
    fn get(&self) -> T;
    /// Updates the value. Returns the list of subscribers that need updating.
    fn set(&self, new_val: T) -> Vec<S>;
    fn subscribe(&self, subscriber_id: S);
}

/// A lightweight, single-threaded implementation for purely local UI state.
/// This prevents atomic overhead (no `Arc`/`Mutex`) for 60fps+ rendering paths.
pub struct LocalState<T, S> {
    value: Rc<RefCell<T>>,
    subscribers: Rc<RefCell<Vec<S>>>,
}

impl<T, S> Clone for LocalState<T, S> {
    fn clone(&self) -> Self {
        Self {
            value: Rc::clone(&self.value),
            subscribers: Rc::clone(&self.subscribers),
        }
    }
}

impl<T: Clone + PartialEq, S: Observer> LocalState<T, S> {
    pub fn new(value: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            subscribers: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl<T: Clone + PartialEq, S: Observer> Observable<T, S> for LocalState<T, S> {
    fn get(&self) -> T {
        self.value.borrow().clone()
    }

    fn set(&self, new_val: T) -> Vec<S> {
        let mut val = self.value.borrow_mut();
        if *val != new_val {
            *val = new_val.clone();
            self.subscribers.borrow().clone()
        } else {
            Vec::new()
        }
    }

    fn subscribe(&self, subscriber_id: S) {
        let mut subs = self.subscribers.borrow_mut();
        if !subs.contains(&subscriber_id) {
            subs.push(subscriber_id);
        }
    }
}
