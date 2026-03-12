# ferrous_ui_macros

Proc-macros para el sistema de UI de Ferrous Engine.

Exporta dos macros:
- **`ui!`** — árbol declarativo de widgets (análogo a JSX / Compose)
- **`#[derive(FerrousWidget)]`** — implementación automática de `FerrousWidgetReflect`

Crate de tipo `proc-macro = true`; depende de `proc-macro2`, `quote` y `syn`.

---

## `ui!` — Árbol declarativo de widgets

Permite construir jerarquías de widgets con sintaxis concisa sin encadenar
llamadas manuales a `ctx.add_child(Box::new(...))`.

### Sintaxis

```
ui! {
    NombreWidget(arg1, arg2) {
        Hijo1(args) {}
        Hijo2(args) {
            Nieto(args) {}
        }
    }
}
```

- El nombre del widget debe ser un tipo que implemente `Widget<App>` y exponga
  `NombreWidget::new(args...)`.
- Los argumentos entre `()` se pasan a `::new(...)`.
- El bloque `{}` es opcional; si está presente, los hijos se añaden al
  `BuildContext` del nodo padre.
- El resultado de la macro es el `NodeId` del nodo raíz.

### Expansión

```rust
// ui! { Button("Aceptar") {} }
// se expande a:
{
    let __id = ctx.add_child(Box::new(Button::new("Aceptar")));
    {
        let mut ctx = BuildContext { tree: ctx.tree, node_id: __id, theme: ctx.theme };
        // hijos aquí...
    }
    __id
}
```

Cada nivel anida su propio `BuildContext` apuntando al nodo padre, de modo
que los hijos se insertan automáticamente en el árbol correcto.

### Ejemplo de uso

```rust
use ferrous_ui_macros::ui;

fn build(&mut self, ctx: &mut BuildContext<MyApp>) {
    ui! {
        Panel() {
            Label("Bienvenido")
            Button("Continuar")
        }
    };
}
```

---

## `#[derive(FerrousWidget)]`

Implementa el trait `FerrousWidgetReflect` para un struct, inspeccionando
cada campo marcado con el atributo `#[prop(...)]`.

### Atributo `#[prop]`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `label = "..."` | `&str` | Nombre visible en el inspector (por defecto: nombre del campo) |
| `category = "..."` | `&str` | Categoría en el panel de propiedades (por defecto: `"General"`) |
| `min = 0.0` | `f32` | Límite inferior para sliders numéricos |
| `max = 100.0` | `f32` | Límite superior |

### Tipos inferidos automáticamente

| Tipo Rust | `PropValue` generado |
|---|---|
| `f32` | `PropValue::Float(value)` |
| `bool` | `PropValue::Bool(value)` |
| `String` | `PropValue::String(value.clone())` |
| `Color` | `PropValue::Color(value.to_array())` |
| `Rect` | `PropValue::Rect(value.to_array())` |
| otros | `PropValue::Bool(false)` (fallback) |

### Ejemplo

```rust
#[derive(FerrousWidget)]
pub struct MyWidget {
    #[prop(label = "Color de fondo", category = "Apariencia")]
    pub bg_color: Color,

    #[prop(label = "Opacidad", min = 0.0, max = 1.0)]
    pub opacity: f32,

    #[prop(label = "Activo")]
    pub enabled: bool,
}
```

Genera automáticamente:

```rust
impl FerrousWidgetReflect for MyWidget {
    fn widget_type_name(&self) -> &'static str { "MyWidget" }

    fn inspect_props(&self) -> Vec<InspectorProp> {
        vec![
            InspectorProp { key: "bg_color", label: "Color de fondo",
                            category: "Apariencia", value: PropValue::Color(...), ... },
            InspectorProp { key: "opacity", label: "Opacidad",
                            range: Some((0.0, 1.0)), value: PropValue::Float(...), ... },
            InspectorProp { key: "enabled", label: "Activo",
                            value: PropValue::Bool(...), ... },
        ]
    }

    fn apply_prop(&mut self, key: &str, value: PropValue) -> bool {
        match key {
            "bg_color" => { /* aplica PropValue::Color */ true }
            "opacity"  => { /* aplica PropValue::Float */ true }
            "enabled"  => { /* aplica PropValue::Bool  */ true }
            _ => false,
        }
    }
}
```

---

## Dependencias

| Crate | Versión | Uso |
|---|---|---|
| `proc-macro2` | `1` | Token streams en macros derive |
| `quote` | `1` | Generación de código Rust |
| `syn` | `1` (full + extra-traits) | Parsing de DeriveInput, campos, atributos |


## Macros Incluidas

### `ui!` (Macro de Construcción Declarativa)

La macro `ui!` permite describir la jerarquía de la interfaz de forma estructural y anidada, eliminando el código imperativo repetitivo.

#### Sintaxis Básica

```rust
let root_id = ui! {
    Panel(Color::RED) {
        Label("Título")
        Button("Click")
    }
};
```

#### Cómo funciona

La macro traduce el árbol declarativo en llamadas directas a `ctx.add_child`. Por ejemplo, el código anterior se expande aproximadamente a:

```rust
{
    let __id = ctx.add_child(Box::new(Panel::new(Color::RED)));
    {
        let mut ctx = BuildContext { ... }; // Contexto transitorio para el subárbol
        ctx.add_child(Box::new(Label::new("Título")));
        ctx.add_child(Box::new(Button::new("Click")));
    }
    __id
}
```

---

## Roadmap del Crate (Fase 5.1 & 7.1)

- [ ] **Soporte de Propiedades con Nombre:** Permitir `Panel(color: #1A1A2E, radius: 8)`.
- [ ] **Bindings Directos:** Soportar `Slider(bind: volume)`.
- [ ] **`#[derive(FerrousWidget)]`:** Macro para habilitar la inspección en tiempo real y serialización para el Ferrous Builder.
- [ ] **Fragmentos:** Soportar condicionales y bucles dentro de la macro `ui!`.

---

## Mejores Prácticas

- Usa `ui!` dentro de la implementación de `Widget::build` para definir los hijos de un componente complejo.
- Dado que es una `proc_macro`, los errores de sintaxis se capturan en tiempo de compilación.
