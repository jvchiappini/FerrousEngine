use crate::{Widget, LayoutContext, Vec2};

// ─── Spacer ──────────────────────────────────────────────────────────────────

/// Widget invisible que empuja a otros widgets en layouts Flex.
pub struct Spacer;

impl<App> Widget<App> for Spacer {
    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        // En Taffy, un widget con size 0 pero flex-grow 1 actuará como spacer.
        // Aquí retornamos 0 para permitir que el estilo flexible haga el trabajo.
        Vec2::ZERO
    }
}