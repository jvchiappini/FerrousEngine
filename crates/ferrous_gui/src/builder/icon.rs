use crate::system::UiSystem;
use ferrous_ui_core::widgets::icon::Icon;
use ferrous_ui_core::NodeId;
use super::WidgetBuilder;

pub struct IconBuilder<App: 'static> {
    widget: Icon,
    common: WidgetBuilder<App>,
}

impl<App: 'static> IconBuilder<App> {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            widget: Icon::new(name.clone()),
            common: WidgetBuilder::new(Icon::new(name)),
        }
    }

    pub fn color(mut self, color: [f32; 4]) -> Self {
        self.widget = self.widget.with_color(color);
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.widget = self.widget.with_size(size);
        // Use the base fields to set size
        self.common.base.width = Some(size);
        self.common.base.height = Some(size);
        self
    }

    pub fn spawn(mut self, ui: &mut UiSystem<App>) -> NodeId {
        self.common.widget = Box::new(self.widget);
        self.common.spawn(ui)
    }
}
