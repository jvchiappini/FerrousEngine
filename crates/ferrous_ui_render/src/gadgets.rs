//! Extracted interactive gadgets/components into a separate module.

use crate::gui_batch::GuiBatch;

impl GuiBatch {
    /// Dibuja un botón y devuelve `true` si fue presionado en este frame.
    #[cfg(feature = "assets")]
    pub fn button(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        mx: f32,
        my: f32,
        clicked: bool,
    ) -> bool {
        let hovered = mx >= x && mx < x + w && my >= y && my < y + h;
        let bg: [f32; 4] = if hovered {
            [0.0, 0.298, 0.612, 1.0] // #0078D4 hover
        } else {
            [0.086, 0.086, 0.086, 1.0] // #161616 idle
        };
        self.rect(x, y, w, h, bg);
        self.draw_text(
            font,
            label,
            [x + 4.0, y + (h - 10.0) * 0.5],
            10.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        hovered && clicked
    }

    /// Igual que `button()` pero con colores personalizados.
    #[cfg(feature = "assets")]
    pub fn button_colored(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        mx: f32,
        my: f32,
        clicked: bool,
        idle_color: [f32; 4],
        hover_color: [f32; 4],
    ) -> bool {
        let hovered = mx >= x && mx < x + w && my >= y && my < y + h;
        let bg = if hovered { hover_color } else { idle_color };
        self.rect(x, y, w, h, bg);
        self.draw_text(
            font,
            label,
            [x + 4.0, y + (h - 10.0) * 0.5],
            10.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        hovered && clicked
    }
}
