use std::sync::{Arc, Mutex};
use crate::NodeId;

/// Un valor que puede ser observado para cambios.
pub struct Observable<T> {
    value: Arc<Mutex<T>>,
    /// Lista de nodos que deben marcarse como sucios cuando este valor cambia.
    subscribers: Arc<Mutex<Vec<NodeId>>>,
}

impl<T: Clone + PartialEq> Observable<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Arc::new(Mutex::new(value)),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }

    /// Actualiza el valor y devuelve la lista de nodos que necesitan actualización.
    pub fn set(&self, new_val: T) -> Vec<NodeId> {
        let mut val = self.value.lock().unwrap();
        if *val != new_val {
            *val = new_val;
            self.subscribers.lock().unwrap().clone()
        } else {
            Vec::new()
        }
    }

    /// Suscribe un nodo para que se marque como sucio cuando el valor cambie.
    pub fn subscribe(&self, node_id: NodeId) {
        let mut subs = self.subscribers.lock().unwrap();
        if !subs.contains(&node_id) {
            subs.push(node_id);
        }
    }
}

/// Implementación simple de un "puente" entre Observables y el UiTree.
pub struct ReactivitySystem {
    /// Cola de nodos que han sido notificados por Observables.
    pub(crate) pending_dirty_nodes: Vec<NodeId>,
}

impl ReactivitySystem {
    pub fn new() -> Self {
        Self {
            pending_dirty_nodes: Vec::new(),
        }
    }

    pub fn notify_change(&mut self, nodes: Vec<NodeId>) {
        self.pending_dirty_nodes.extend(nodes);
    }

    pub fn apply<App>(&mut self, tree: &mut crate::UiTree<App>) {
        for id in self.pending_dirty_nodes.drain(..) {
            tree.mark_paint_dirty(id);
        }
    }
}
