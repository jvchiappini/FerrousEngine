use crate::Widget;

// ─── PlaceholderWidget ───────────────────────────────────────────────────────

/// Widget vacío para uso estructural o provisional en el árbol.
pub struct PlaceholderWidget;

impl<App> Widget<App> for PlaceholderWidget {}
