use crate::layout::RenderCommand;
use crate::widget::Widget;

/// A lightweight widget representing a rectangular viewport area. The
/// widget does not draw anything itself, but it participates in the focus
/// system so that the rest of the application can know when the user has
/// clicked inside the 3D viewport and hence the viewport should capture
/// input instead of the UI.
#[derive(Debug, Clone)]
pub struct ViewportWidget {
    pub rect: [f32; 4],
    pub focused: bool,
}

impl ViewportWidget {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            focused: false,
        }
    }

    pub fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }
}

impl Widget for ViewportWidget {
    fn collect(&self, _cmds: &mut Vec<RenderCommand>) {
        // viewport doesn't render anything as part of the UI
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        self.hit(mx, my)
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            self.focused = self.hit(mx, my);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_and_focus() {
        let mut vp = ViewportWidget::new(0.0, 0.0, 100.0, 50.0);
        assert!(vp.hit(10.0, 10.0));
        assert!(!vp.hit(-1.0, 0.0));
        vp.mouse_input(10.0, 10.0, true);
        assert!(vp.focused);
        vp.mouse_input(200.0, 200.0, true);
        assert!(!vp.focused);
    }
}
