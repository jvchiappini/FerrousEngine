use crate::elements::cube::Cube;
// use crate::Transform;

/// A very small set of types that can be placed into a scene.  at the moment
/// the only supported element is a cube; future releases can add lights,
/// cameras, meshes, etc.
#[derive(Debug, Clone)]
pub enum Element {
    Cube(Cube),
}

/// A trivial "world" which owns a collection of scene elements.  it is
/// deliberately much simpler than the old `elements::World`; instead of
/// entities and component maps it simply keeps a vector of entries and
/// exposes a lightweight handle that callers can use for updates.  the
/// renderer and editor rely on this structure, so it contains basic support
/// for transforms and an optional render handle.
#[derive(Debug, Default)]
pub struct World {
    elements: Vec<ElementEntry>,
}

/// Handle pointing to a specific element in a `World`.
pub type Handle = usize;

#[derive(Debug)]
struct ElementEntry {
    element: Element,
    render_handle: Option<usize>,
}

impl World {
    /// Create an empty world ready to accept elements.
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
        }
    }

    /// Add a new element to the scene.  the element is taken by value and
    /// stored internally; the returned handle may be used to modify the
    /// element later (for example to change its transform or attach a render
    /// handle).
    pub fn add_element(&mut self, element: Element) -> Handle {
        let handle = self.elements.len();
        self.elements.push(ElementEntry {
            element,
            render_handle: None,
        });
        handle
    }

    /// Convenience helper for adding a cube without having to mention the
    /// `Element` enum directly.
    pub fn add_cube(&mut self, cube: Cube) -> Handle {
        self.add_element(Element::Cube(cube))
    }

    /// Update the position of a cube element.  other element types are
    /// currently ignored.
    pub fn set_position(&mut self, handle: Handle, pos: glam::Vec3) {
        if let Some(entry) = self.elements.get_mut(handle) {
            #[allow(irrefutable_let_patterns)]
            if let Element::Cube(ref mut c) = entry.element {
                c.position = pos;
            }
        }
    }

    /// Read-only access to an element's position.
    pub fn position(&self, handle: Handle) -> Option<glam::Vec3> {
        self.elements.get(handle).and_then(|e| match &e.element {
            Element::Cube(c) => Some(c.position),
        })
    }

    /// Iterate over all elements in the world.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.iter().map(|e| &e.element)
    }

    /// Access a single element by its handle.
    pub fn element(&self, handle: Handle) -> Option<&Element> {
        self.elements.get(handle).map(|e| &e.element)
    }

    /// Iterate over elements together with their handles.
    pub fn elements_with_handles(&self) -> impl Iterator<Item = (Handle, &Element)> {
        self.elements
            .iter()
            .enumerate()
            .map(|(i, e)| (i, &e.element))
    }

    /// Store the renderer's handle associated with the given element.  this
    /// is typically called by `Renderer::sync_world` when an object is first
    /// created on the GPU.
    pub fn set_render_handle(&mut self, handle: Handle, rh: usize) {
        if let Some(entry) = self.elements.get_mut(handle) {
            entry.render_handle = Some(rh);
        }
    }

    /// Retrieve a previously stored renderer handle, if any.
    pub fn render_handle(&self, handle: Handle) -> Option<usize> {
        self.elements.get(handle).and_then(|e| e.render_handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_cube() {
        let mut world = World::new();
        let cube = Cube::default();
        let h = world.add_element(Element::Cube(cube));
        let elems: Vec<_> = world.elements().collect();
        assert_eq!(elems.len(), 1);
        #[allow(irrefutable_let_patterns)]
        if let Element::Cube(c) = &elems[0] {
            assert_eq!(c.size, 1.0);
        } else {
            panic!("expected cube");
        }
        // newly added cube should have default position zero and a valid id
        assert_eq!(world.position(h).unwrap(), glam::Vec3::ZERO);
        #[allow(irrefutable_let_patterns)]
        if let Element::Cube(c) = &elems[0] {
            assert!(c.id != 0);
            assert!(c.name.contains(&c.id.to_string()));
        }
    }
}
