use crate::{layout::Rect, RenderCommand, Widget};

/// Eje de resize: horizontal (arrastra en X) o vertical (arrastra en Y).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ResizeAxis {
    Horizontal, // drag en X → cambia ancho
    Vertical,   // drag en Y → cambia alto
}

/// "Handle" invisible que el usuario puede arrastrar para redimensionar
/// un panel adyacente.  Funciona igual que los "splitters" de los IDEs.
#[derive(Clone, Copy, Debug)]
pub struct PanelResizeHandle {
    /// Rect de la zona de detección [x, y, w, h] en pixels de ventana.
    pub rect: [f32; 4],
    /// Valor actual (posición del splitter en su eje).
    pub value: f32,
    /// Rango permitido [min, max].
    pub min: f32,
    pub max: f32,
    /// Eje de resize.
    pub axis: ResizeAxis,
    /// Color del indicador visual (visible al hacer hover).
    pub hover_color: [f32; 4],
    /// true mientras el usuario está arrastrando.
    pub dragging: bool,
    /// true si el cursor está sobre el handle.
    pub hovered: bool,
    /// Posición del cursor en el eje al iniciar el drag (interno).
    drag_start_cursor: f32,
    /// Valor al iniciar el drag (interno).
    drag_start_value: f32,
}

impl PanelResizeHandle {
    /// `x`, `y`, `w`, `h` — rect de hit-test (normalmente una franja de 6-8 px).
    /// `value` — posición inicial del splitter.
    /// `min`, `max` — rango de valores permitidos.
    /// `axis` — ResizeAxis::Horizontal o ResizeAxis::Vertical.
    pub fn new(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        value: f32,
        min: f32,
        max: f32,
        axis: ResizeAxis,
    ) -> Self {
        let mut handle = Self {
            rect: [x, y, w, h],
            value: value.clamp(min, max),
            min,
            max,
            axis,
            hover_color: [0.0, 0.5, 1.0, 0.5], // azul semitransparente por defecto
            dragging: false,
            hovered: false,
            drag_start_cursor: 0.0,
            drag_start_value: value.clamp(min, max),
        };
        // position rect according to value on the appropriate axis
        match axis {
            ResizeAxis::Horizontal => {
                handle.rect[0] = handle.value - handle.rect[2] / 2.0;
            }
            ResizeAxis::Vertical => {
                handle.rect[1] = handle.value - handle.rect[3] / 2.0;
            }
        }
        handle
    }

    /// Color del indicador visual al hacer hover (default: azul semitransparente).
    pub fn with_hover_color(mut self, color: [f32; 4]) -> Self {
        self.hover_color = color;
        self
    }
}

impl Widget for PanelResizeHandle {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        if self.hovered || self.dragging {
            cmds.push(RenderCommand::Quad {
                rect: Rect {
                    x: self.rect[0],
                    y: self.rect[1],
                    width: self.rect[2],
                    height: self.rect[3],
                },
                color: self.hover_color,
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    fn hit(&self, mx: f64, my: f64) -> bool {
        let x = mx as f32;
        let y = my as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }

    fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            if self.hovered {
                self.dragging = true;
                let cursor = if self.axis == ResizeAxis::Horizontal {
                    mx as f32
                } else {
                    my as f32
                };
                self.drag_start_cursor = cursor;
                self.drag_start_value = self.value;
            }
        } else {
            self.dragging = false;
        }
    }

    fn mouse_move(&mut self, mx: f64, my: f64) {
        // update hover state
        self.hovered = self.hit(mx, my);

        if self.dragging {
            let cursor = if self.axis == ResizeAxis::Horizontal {
                mx as f32
            } else {
                my as f32
            };
            let delta = cursor - self.drag_start_cursor;
            let new_val = (self.drag_start_value + delta).clamp(self.min, self.max);
            if (new_val - self.value).abs() > f32::EPSILON {
                self.value = new_val;
            }
            // reposition rect so it stays centred on value
            match self.axis {
                ResizeAxis::Horizontal => {
                    self.rect[0] = self.value - self.rect[2] / 2.0;
                }
                ResizeAxis::Vertical => {
                    self.rect[1] = self.value - self.rect[3] / 2.0;
                }
            }
        }
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
    }
}
