//! `StyleBuilder` — API fluent para definir estilos de layout de forma legible.
//!
//! Proporciona una alternativa encadenada al constructor verbose de `Style { ... }`.
//! Inspirado en la ergonomía de CSS y la API de SwiftUI.
//!
//! # Ejemplo
//!
//! ```rust
//! use ferrous_ui_core::StyleBuilder;
//!
//! let style = StyleBuilder::new()
//!     .fill_width()
//!     .height_px(48.0)
//!     .padding_all(8.0)
//!     .row()
//!     .center_items()
//!     .gap_px(12.0)
//!     .build();
//! ```

use crate::{Style, Units, DisplayMode, Alignment, Position, RectOffset};

/// Constructor fluent para [`Style`].
///
/// `StyleBuilder` permite encadenar modificadores legibles en lugar de rellenar
/// los campos de `Style` manualmente. Produce un `Style` inmutable al invocar [`build`](StyleBuilder::build).
pub struct StyleBuilder {
    style: Style,
}

impl StyleBuilder {
    /// Crea un `StyleBuilder` con todos los valores por defecto.
    pub fn new() -> Self {
        Self { style: Style::default() }
    }

    // ─── Tamaño ───────────────────────────────────────────────────────────────

    /// Ancho fijo en píxeles.
    pub fn width_px(mut self, px: f32) -> Self {
        self.style.size.0 = Units::Px(px);
        self
    }

    /// Alto fijo en píxeles.
    pub fn height_px(mut self, px: f32) -> Self {
        self.style.size.1 = Units::Px(px);
        self
    }

    /// Ancho como porcentaje del contenedor padre (0.0–100.0).
    pub fn width_pct(mut self, pct: f32) -> Self {
        self.style.size.0 = Units::Percentage(pct);
        self
    }

    /// Alto como porcentaje del contenedor padre (0.0–100.0).
    pub fn height_pct(mut self, pct: f32) -> Self {
        self.style.size.1 = Units::Percentage(pct);
        self
    }

    /// Ocupa todo el ancho disponible del contenedor (equivalente a `width: 100%`).
    pub fn fill_width(mut self) -> Self {
        self.style.size.0 = Units::Percentage(100.0);
        self
    }

    /// Ocupa todo el alto disponible del contenedor (equivalente a `height: 100%`).
    pub fn fill_height(mut self) -> Self {
        self.style.size.1 = Units::Percentage(100.0);
        self
    }

    /// Ocupa todo el espacio disponible (ancho y alto al 100%).
    pub fn fill(self) -> Self {
        self.fill_width().fill_height()
    }

    /// Factor flex para repartir el espacio sobrante (equivalente a `flex-grow`).
    /// Un valor de `1.0` es equivalente a `flex: 1` en CSS.
    pub fn flex(mut self, factor: f32) -> Self {
        self.style.size.0 = Units::Flex(factor);
        self
    }

    /// Dimensiones automáticas (el layout infiere el tamaño por el contenido).
    pub fn size_auto(mut self) -> Self {
        self.style.size = (Units::Auto, Units::Auto);
        self
    }

    // ─── Padding ──────────────────────────────────────────────────────────────

    /// Relleno uniforme en todos los lados.
    pub fn padding_all(mut self, px: f32) -> Self {
        self.style.padding = RectOffset::all(px);
        self
    }

    /// Relleno horizontal (izquierda + derecha) y vertical (arriba + abajo).
    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.style.padding = RectOffset { left: x, right: x, top: y, bottom: y };
        self
    }

    /// Relleno completo especificando los cuatro lados individualmente.
    pub fn padding(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.style.padding = RectOffset { left, right, top, bottom };
        self
    }

    // ─── Margen ───────────────────────────────────────────────────────────────

    /// Margen uniforme en todos los lados.
    pub fn margin_all(mut self, px: f32) -> Self {
        self.style.margin = RectOffset::all(px);
        self
    }

    /// Margen horizontal y vertical.
    pub fn margin_xy(mut self, x: f32, y: f32) -> Self {
        self.style.margin = RectOffset { left: x, right: x, top: y, bottom: y };
        self
    }

    // ─── Display Mode ─────────────────────────────────────────────────────────

    /// Dispone los hijos en una fila horizontal (Flex Row).
    pub fn row(mut self) -> Self {
        self.style.display = DisplayMode::FlexRow;
        self
    }

    /// Dispone los hijos en una columna vertical (Flex Column).
    pub fn column(mut self) -> Self {
        self.style.display = DisplayMode::FlexColumn;
        self
    }

    /// Modo block estándar (hijos apilados verticalmente sin Flexbox).
    pub fn block(mut self) -> Self {
        self.style.display = DisplayMode::Block;
        self
    }

    // ─── Alineación ───────────────────────────────────────────────────────────

    /// Centra los hijos en ambos ejes.
    pub fn center_items(mut self) -> Self {
        self.style.alignment = Alignment::Center;
        self
    }

    /// Alinea los hijos al inicio del eje principal.
    pub fn start_items(mut self) -> Self {
        self.style.alignment = Alignment::Start;
        self
    }

    /// Alinea los hijos al final del eje principal.
    pub fn end_items(mut self) -> Self {
        self.style.alignment = Alignment::End;
        self
    }

    /// Estira los hijos para llenar el eje transversal.
    pub fn stretch_items(mut self) -> Self {
        self.style.alignment = Alignment::Stretch;
        self
    }

    // ─── Gap ─────────────────────────────────────────────────────────────────
    //
    // NOTA: `ferrous_ui_core::Style` todavía no tiene un campo `gap`. Cuando se
    // añada, este método populará ese campo. Por ahora es un no-op reservado.

    /// Espaciado entre hijos en layouts Flex (actualmente reservado — requiere
    /// que `Style` exponga un campo `gap` en futuras versiones).
    pub fn gap_px(self, _px: f32) -> Self {
        // TODO: añadir campo `gap: f32` a `Style` y mapearlo a `taffy::Style::gap`
        self
    }

    // ─── Posicionamiento ──────────────────────────────────────────────────────

    /// Posicionamiento relativo al flujo normal del layout.
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self
    }

    /// Posicionamiento absoluto respecto al ancestro más cercano con `Position::Relative`.
    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    /// Offset superior para posicionamiento absoluto.
    pub fn top(mut self, px: f32) -> Self {
        self.style.offsets.top = px;
        self
    }

    /// Offset inferior para posicionamiento absoluto.
    pub fn bottom(mut self, px: f32) -> Self {
        self.style.offsets.bottom = px;
        self
    }

    /// Offset izquierdo para posicionamiento absoluto.
    pub fn left(mut self, px: f32) -> Self {
        self.style.offsets.left = px;
        self
    }

    /// Offset derecho para posicionamiento absoluto.
    pub fn right(mut self, px: f32) -> Self {
        self.style.offsets.right = px;
        self
    }

    // ─── Overflow ─────────────────────────────────────────────────────────────

    /// El contenido sobresale del nodo (comportamiento por defecto).
    pub fn overflow_visible(mut self) -> Self {
        self.style.overflow = crate::Overflow::Visible;
        self
    }

    /// El contenido se recorta rígidamente según las dimensiones del nodo.
    pub fn clip(mut self) -> Self {
        self.style.overflow = crate::Overflow::Hidden;
        self
    }

    /// El contenido se recorta y se habilita el desplazamiento.
    pub fn scroll(mut self) -> Self {
        self.style.overflow = crate::Overflow::Scroll;
        self
    }

    // ─── Build ────────────────────────────────────────────────────────────────

    /// Consume el builder y devuelve el [`Style`] construido.
    pub fn build(self) -> Style {
        self.style
    }
}

impl Default for StyleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extensión de conveniencia para crear un `StyleBuilder` directamente desde `Style`.
pub trait StyleExt {
    /// Inicia un `StyleBuilder` a partir del estilo actual.
    fn builder() -> StyleBuilder;
}

impl StyleExt for Style {
    fn builder() -> StyleBuilder {
        StyleBuilder::new()
    }
}
