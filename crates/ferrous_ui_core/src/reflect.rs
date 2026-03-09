//! # `FerrousReflect` — Sistema de Reflexión para Ferrous Builder
//!
//! Este módulo define los tipos y traits necesarios para que los widgets sean
//! "inspeccionables" desde el editor visual (Ferrous Builder). Permite acceder a
//! las propiedades de un widget por nombre, editarlas en caliente y serializarlas
//! al formato de archivo `.fui`.

use serde::{Serialize, Deserialize};
use crate::Rect;

// ─── PropValue ────────────────────────────────────────────────────────────────

/// Valores de propiedad permitidos para edición en el Inspector del Builder.
/// Es un subconjunto de tipos serializables que la UI puede manipular fácilmente.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum PropValue {
    /// Texto libre o ID de recurso.
    String(String),
    /// Flotante de precisión simple (e.g. radios, opacidad, tamaños).
    Float(f32),
    /// Valor booleano.
    Bool(bool),
    /// Color RGBA en formato `[f32; 4]`.
    Color([f32; 4]),
    /// Rectángulo en formato `[x, y, w, h]`.
    Rect([f32; 4]),
    /// Índice entero (e.g. selección de dropdown, capas).
    Int(i32),
}

impl Default for PropValue {
    fn default() -> Self {
        PropValue::Bool(false)
    }
}

// ─── InspectorProp ────────────────────────────────────────────────────────────

/// Metadatos de una propiedad para ser mostrada en el panel Inspector del Builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectorProp {
    /// Nombre interno de la propiedad (debe coincidir con el campo de la struct).
    pub key: String,
    /// Etiqueta legible para el usuario final en la UI.
    pub label: String,
    /// Categoría para agrupar en el Inspector (e.g. "Layout", "Apariencia").
    pub category: String,
    /// Valor actual de la propiedad.
    pub value: PropValue,
    /// Rango permitido para valores numéricos (opcional).
    pub range: Option<(f32, f32)>,
    /// Tooltip descriptivo (opcional).
    pub tooltip: Option<String>,
}

// ─── FerrousWidgetReflect ──────────────────────────────────────────────────────

/// Trait que todo widget inspeccionable debe implementar.
/// Normalmente se genera automáticamente mediante `#[derive(FerrousWidget)]`.
pub trait FerrousWidgetReflect {
    /// Devuelve el nombre único del tipo de widget (e.g. "Button").
    fn widget_type_name(&self) -> &'static str;

    /// Lista de propiedades editables por el usuario en el Builder.
    fn inspect_props(&self) -> Vec<InspectorProp>;

    /// Aplica un nuevo valor a una propiedad identificada por su `key`.
    /// Devuelve `true` si la propiedad existía y el valor fue aplicado.
    fn apply_prop(&mut self, key: &str, value: PropValue) -> bool;
}

// ─── WidgetFactory ────────────────────────────────────────────────────────────

type WidgetCreator<App> = Box<dyn Fn() -> Box<dyn crate::Widget<App>> + Send + Sync>;

/// Factoría para instanciar widgets a partir de sus nombres (strings).
/// Esencial para cargar archivos `.fui` y para el Builder.
pub struct WidgetFactory<App> {
    creators: std::collections::HashMap<String, WidgetCreator<App>>,
}

impl<App: 'static + Send + Sync> WidgetFactory<App> {
    pub fn new() -> Self {
        Self {
            creators: std::collections::HashMap::new(),
        }
    }

    /// Registra un tipo de widget en la factoría.
    pub fn register<W, F>(&mut self, name: &str, creator: F)
    where
        W: crate::Widget<App> + 'static,
        F: Fn() -> Box<dyn crate::Widget<App>> + 'static + Send + Sync,
    {
        self.creators.insert(name.to_string(), Box::new(creator));
    }

    /// Crea una instancia del widget por su nombre.
    pub fn create(&self, name: &str) -> Option<Box<dyn crate::Widget<App>>> {
        self.creators.get(name).map(|f| f())
    }

    /// Carga un subárbol completo recursivamente desde un `FuiNode`.
    pub fn instantiate_tree(
        &self,
        tree: &mut crate::UiTree<App>,
        fui: &FuiNode,
        parent: Option<crate::NodeId>,
    ) -> Option<crate::NodeId> {
        let mut widget = self.create(&fui.widget)?;
        
        // Aplicar propiedades si el widget soporta reflexión
        if let Some(reflect) = widget.reflect_mut() {
            for (key, val) in &fui.props {
                reflect.apply_prop(key, val.clone());
            }
        }

        let id = tree.add_node(widget, parent);
        tree.set_node_style(id, fui.style.clone());

        for child_fui in &fui.children {
            self.instantiate_tree(tree, child_fui, Some(id));
        }

        Some(id)
    }
}

// ─── Hot-Reload Support ───────────────────────────────────────────────────────

/// Representación serializada de un nodo de la UI para el formato `.fui`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuiNode {
    /// Tipo de widget (debe estar registrado en la factory del Builder).
    pub widget: String,
    /// Propiedades actuales del widget.
    pub props: Vec<(String, PropValue)>,
    /// Estilo del nodo (layout, padding, etc.).
    pub style: crate::Style,
    /// Nodos hijos anidados.
    pub children: Vec<FuiNode>,
}

impl FuiNode {
    /// Serializa el subárbol a una cadena en formato RON.
    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .expect("Fallo al serializar FuiNode a RON")
    }

    /// Carga un subárbol desde una cadena RON.
    pub fn from_ron(ron_str: &str) -> Result<Self, ron::Error> {
        ron::from_str(ron_str).map_err(|e| e.code)
    }
}

/// Registra todos los widgets estándar del motor en la factoría proporcionada.
pub fn register_core_widgets<App: 'static + Send + Sync>(factory: &mut WidgetFactory<App>) {
    factory.register::<crate::widgets::Button<App>, _>("Button", || Box::new(crate::widgets::Button::<App>::new("Button")));
    // TODO: Registrar el resto de widgets (Label, TextInput, etc.)
}
