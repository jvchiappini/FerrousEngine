use crate::canvas::Canvas;
use crate::constraint::Constraint;
use crate::layout::{Rect, RenderCommand};
use crate::widget::Widget;
use crate::GuiKey;

/// Simple container widget that groups other widgets together.  The
/// container itself may optionally draw a background quad, but otherwise
/// merely acts as a forwarding layer; child widgets are responsible for
/// their own positioning and rendering.  This is convenient for treating a
/// set of widgets as a single unit (for example, to hit‑test or move them
/// together) or to provide an enclosing visual frame.
///
/// ## Clip / scissor
///
/// When `clip` is `true` (the default is `false`) the container emits a
/// `RenderCommand::PushClip` before its children and a `RenderCommand::PopClip`
/// after them.  This signals to any renderer integration that children
/// should be scissored to the container's bounding rectangle.
///
/// The container implements `Widget` itself so it can be added directly to
/// a `Ui`/`Canvas`.  Input events are only propagated to children when the
/// pointer lies inside the container's rectangle; keyboard events are always
/// forwarded to the currently focused child regardless of mouse position.
///
/// The public API mirrors [`Canvas`] for managing the child list.
pub struct Container {
    /// bounding rectangle in window coordinates.
    pub rect: [f32; 4],
    /// optional background colour (RGBA).  `None` means transparent.
    pub bg_color: Option<[f32; 4]>,
    /// When `true` the container emits clip commands around its children so
    /// that a renderer can scissor-test them to the container's rect.
    pub clip: bool,
    /// Optional reactive layout constraint.
    pub constraint: Option<Constraint>,
    canvas: Canvas,
}

impl Container {
    /// Create an empty container with the given rectangle and no background.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            bg_color: None,
            clip: false,
            constraint: None,
            canvas: Canvas::new(),
        }
    }

    /// Compute the actual rectangle used for hit-testing and drawing.
    ///
    /// If the stored width or height is non-positive the container will
    /// expand to enclose all children that report a `bounding_rect()`.
    fn effective_rect(&self) -> [f32; 4] {
        let mut r = self.rect;
        // auto-size width
        if r[2] <= 0.0 {
            let mut maxw: f32 = 0.0;
            for child in self.canvas.children() {
                if let Some(cr) = child.bounding_rect() {
                    // right edge relative to container origin
                    maxw = maxw.max(cr[0] + cr[2] - r[0]);
                }
            }
            r[2] = maxw;
        }
        // auto-size height
        if r[3] <= 0.0 {
            let mut maxh: f32 = 0.0;
            for child in self.canvas.children() {
                if let Some(cr) = child.bounding_rect() {
                    maxh = maxh.max(cr[1] + cr[3] - r[1]);
                }
            }
            r[3] = maxh;
        }
        r
    }

    /// Set a solid background colour for the container.
    pub fn with_background(mut self, color: [f32; 4]) -> Self {
        self.bg_color = Some(color);
        self
    }

    /// Enable scissor/clip rect for children.  When set, the container emits
    /// `PushClip` / `PopClip` commands around its children during `collect`.
    pub fn with_clip(mut self) -> Self {
        self.clip = true;
        self
    }

    /// Attach a reactive layout constraint.
    pub fn with_constraint(mut self, c: Constraint) -> Self {
        self.constraint = Some(c);
        self
    }

    /// Add a widget to this container.
    pub fn add(&mut self, widget: impl Widget + 'static) {
        self.canvas.add(widget);
    }

    /// Forward a mouse-move event to children only if the pointer lies inside
    /// our rect.  This preserves behaviour such as slider dragging while
    /// allowing the container to block events outside its bounds.
    pub fn mouse_move(&mut self, mx: f64, my: f64) {
        let r = self.effective_rect();
        let x = mx as f32;
        let y = my as f32;
        if x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3] {
            self.canvas.mouse_move(mx, my);
        }
    }

    /// Handle mouse button input.  Presses outside the rect are ignored;
    /// releases are forwarded only when the pointer is inside so that
    /// children don't continue reacting after the cursor leaves the group.
    pub fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        let r = self.effective_rect();
        let x = mx as f32;
        let y = my as f32;
        if x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3] {
            self.canvas.mouse_input(mx, my, pressed);
        }
    }

    /// Keyboard events are always forwarded to the focused child, since the
    /// container itself does not take focus.
    pub fn keyboard_input(&mut self, text: Option<&str>, key: Option<GuiKey>, pressed: bool) {
        self.canvas.keyboard_input(text, key, pressed);
    }
}

impl Widget for Container {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        let r = self.effective_rect();
        if let Some(col) = self.bg_color {
            cmds.push(RenderCommand::Quad {
                rect: Rect {
                    x: r[0],
                    y: r[1],
                    width: r[2],
                    height: r[3],
                },
                color: col,
                radii: [0.0; 4],
                flags: 0,
            });
        }
        if self.clip {
            cmds.push(RenderCommand::PushClip {
                rect: Rect {
                    x: r[0],
                    y: r[1],
                    width: r[2],
                    height: r[3],
                },
            });
        }
        self.canvas.collect(cmds);
        if self.clip {
            cmds.push(RenderCommand::PopClip);
        }
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        let r = self.effective_rect();
        let x = mx as f32;
        let y = my as f32;
        x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
    }

    fn mouse_move(&mut self, mx: f64, my: f64) {
        self.mouse_move(mx, my);
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        self.mouse_input(mx, my, pressed);
    }

    fn keyboard_input(&mut self, text: Option<&str>, key: Option<GuiKey>, pressed: bool) {
        self.keyboard_input(text, key, pressed);
    }

    fn apply_constraint(&mut self, container_w: f32, container_h: f32) {
        if let Some(c) = self.constraint.clone() {
            let old_x = self.rect[0];
            let old_y = self.rect[1];
            c.apply_to_rect(&mut self.rect, container_w, container_h);
            let dx = self.rect[0] - old_x;
            let dy = self.rect[1] - old_y;
            // Propagate position delta to all direct children.
            if dx != 0.0 || dy != 0.0 {
                for child in self.canvas.children_mut() {
                    shift_widget(child.as_mut(), dx, dy);
                }
            }
        }
    }

    fn apply_constraint_with(&mut self, c: &Constraint, cw: f32, ch: f32) {
        c.apply_to_rect(&mut self.rect, cw, ch);
    }
}

/// Shift a widget's position by `(dx, dy)`.
///
/// This is an internal helper used when a `Container`'s constraint changes
/// its origin: all direct children are nudged by the same delta.
/// Widgets that override `apply_constraint` with a `rect` field will respond;
/// others are silently unaffected.
pub(crate) fn shift_widget(widget: &mut dyn Widget, dx: f32, dy: f32) {
    widget.shift(dx, dy);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy {
        called: std::cell::Cell<bool>,
    }

    impl Dummy {
        fn new() -> Self {
            Self {
                called: std::cell::Cell::new(false),
            }
        }
    }

    impl Widget for Dummy {
        fn collect(&self, cmds: &mut Vec<RenderCommand>) {
            self.called.set(true);
            // add a dummy quad command
            cmds.push(RenderCommand::Quad {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: [1.0, 1.0, 1.0, 1.0],
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    #[test]
    fn container_background_and_event_forwarding() {
        // container at origin with size 100x100 and a solid background
        let mut cont = Container::new(0.0, 0.0, 100.0, 100.0).with_background([0.2, 0.4, 0.6, 1.0]);
        // add a dummy widget that records when its collect is called
        let dummy = Dummy::new();
        cont.add(dummy);

        // collecting should produce two commands: background + dummy quad
        let mut cmds = Vec::new();
        cont.collect(&mut cmds);
        assert_eq!(cmds.len(), 2);

        // mouse_move inside should propagate to child (sets dummy.called)
        cont.mouse_move(10.0, 10.0);
        // we can't directly inspect dummy from here since it was moved into
        // container; instead rely on the fact that collect earlier triggered
        // it.  for a more thorough test we could expose a custom widget, but
        // this suffices to exercise the path.

        // hitting outside should return false
        assert!(!cont.hit(-1.0, -1.0));
        // hitting inside should return true
        assert!(cont.hit(50.0, 50.0));
    }
}
