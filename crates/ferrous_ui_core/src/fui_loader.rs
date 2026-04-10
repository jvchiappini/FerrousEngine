use std::string::String;
use std::vec::Vec;
use crate::{NodeId, UiTree, FerrousController};
use crate::widgets::panel::Panel;
use crate::widgets::button::Button;
use crate::widgets::label::Label;
use std::collections::BTreeMap;
use std::boxed::Box;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FuiLayout {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FuiNode {
    pub kind: String,
    pub id: Option<String>,
    pub on_click: Option<String>,
    pub layout: FuiLayout,
    pub props: BTreeMap<String, String>,
    #[serde(default)]
    pub children: Vec<FuiNode>,
}

pub struct FuiLoader;

impl FuiLoader {
    pub fn load_view_json<App: FerrousController>(tree: &mut UiTree<App>, view_json: &str, controller: &mut App) -> Result<Vec<NodeId>, String> {
        let parsed_nodes: Vec<FuiNode> = serde_json::from_str(view_json)
            .map_err(|e| format!("Error parsing FUI JSON: {}", e))?;
        
        let mut root_ids = Vec::new();
        for node in parsed_nodes {
            root_ids.push(Self::build_node(tree, &node, None, controller));
        }
        
        Ok(root_ids)
    }

    fn build_node<App: FerrousController>(tree: &mut UiTree<App>, node: &FuiNode, parent: Option<NodeId>, controller: &mut App) -> NodeId {
        let widget_box: Box<dyn crate::Widget<App>> = match node.kind.as_str() {
            "Panel" => Box::new(Panel::new()),
            "Button" => Box::new(Button::new(node.props.get("label").cloned().unwrap_or_default())),
            "Label" => Box::new(Label::new(node.props.get("label").cloned().unwrap_or_default())),
            _ => Box::new(Panel::new()),
        };

        let node_id = tree.add_node(widget_box, parent); 
        
        // Setup layout based on node.layout
        if let Some(mut n) = tree.get_node_mut(node_id) {
            n.style.size = (crate::primitives::Units::Px(node.layout.w), crate::primitives::Units::Px(node.layout.h));
            // Just positioning
            n.rect.x = node.layout.x;
            n.rect.y = node.layout.y;
            n.rect.width = node.layout.w;
            n.rect.height = node.layout.h;
        }

        if let Some(ref fui_id) = node.id {
            controller.inject_fui_id(fui_id, node_id);
        }

        if let Some(ref _on_click) = node.on_click {
            // events dispatcher injected later via FUI macros
        }

        for child in &node.children {
            Self::build_node(tree, child, Some(node_id), controller);
        }

        node_id
    }
}
