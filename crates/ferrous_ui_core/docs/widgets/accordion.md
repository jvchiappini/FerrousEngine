# Accordion

`Accordion` es un contenedor que puede expandirse o colapsarse haciendo clic en su cabecera. Ideal para organizar opciones avanzadas, FAQs o cualquier contenido que deba mantenerse oculto por defecto.

> **Import** — `ferrous_ui_core::Accordion`

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `Accordion::new(title)` | Crea un accordion colapsado con el título dado. |
| `.with_content(Box<dyn Widget<App>>)` | Establece el widget que se muestra al expandir. |
| `.expanded(bool)` | Estado inicial: `true` = expandido, `false` = colapsado (por defecto). |
| `.with_header_color(Color)` | Color personalizado para la barra de cabecera. |

---

## Ejemplo de Uso

```rust
use ferrous_ui_core::{Accordion, Label, StyleBuilder, StyleExt};

// Accordion simple, colapsado
let acc = Accordion::<MyApp>::new("Configuración avanzada")
    .with_content(Box::new(Label::new("Opciones adicionales aquí")))
    .expanded(false);

let id = tree.add_node(Box::new(acc), Some(root_id));
tree.set_node_style(id, StyleBuilder::new().fill_width().build());
```

```rust
// Accordion abierto con color de cabecera personalizado
let acc = Accordion::<MyApp>::new("Shaders")
    .with_content(Box::new(shader_panel))
    .expanded(true)
    .with_header_color(Color::hex("#2D2D44"));
```

---

## Comportamiento

- **Clic en cabecera**: Invierte `is_expanded`. El área de contenido cambia a `height: Auto` (expandido) o `height: 0px` (colapsado). `mark_layout_dirty` propaga el cambio al motor de layout.
- **Múltiples Accordions**: Cada instancia es independiente. Para un comportamiento "exclusivo" (solo uno abierto a la vez), gestiona el estado con un `Observable<usize>` en tu `App`.

---

## Arquitectura Interna

```
Accordion (root — FlexColumn)
├── Panel (header — h=40px, clickable)   ← clic aquí para toggle
│   └── [dibuja icono ▶/▼ + título en draw()]
└── Panel "181825" (content_area — overflow: hidden)
    └── <widget de contenido>            ← oculto cuando height=0
```

> [!NOTE]
> El `Accordion` usa `Overflow::Hidden` + `height: Px(0)` para ocultar el contenido,
> no lo elimina del árbol. Esto significa que el widget de contenido mantiene su estado
> (valores de formularios, posición de scroll, etc.) entre aperturas.

---

## Estilo Visual

| Elemento | Apariencia |
|----------|-----------|
| Cabecera | `theme.surface_elevated` (o color personalizado), radio `border_radius` arriba |
| Icono toggle | `▶` (colapsado) / `▼` (expandido) en `theme.on_surface_muted` |
| Título | `theme.on_surface`, tamaño `font_size_base` |
| Área de contenido | `#181825`, `padding: 8px` |

> [!TIP]
> Para anidar Accordions, simplemente pasa otro `Accordion` como `content`:
> ```rust
> let inner = Accordion::<App>::new("Sub-sección").with_content(Box::new(...));
> let outer = Accordion::<App>::new("Sección").with_content(Box::new(inner));
> ```
