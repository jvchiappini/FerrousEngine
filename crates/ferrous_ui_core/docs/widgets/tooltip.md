# Tooltip

`Tooltip` es un widget **envolvente** que muestra un popup de texto informativo cuando el cursor permanece sobre su hijo durante un tiempo configurable. El popup se posiciona automáticamente para permanecer dentro del viewport.

> **Import** — `ferrous_ui_core::Tooltip`

---

## Particularidad de diseño

El tooltip **no es un nodo separado del árbol**: sus `RenderCommand` se emiten directamente en `draw()` del widget envoltura. Esto garantiza que el popup siempre aparezca *encima* de cualquier otro contenido sin reordenar el árbol.

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `Tooltip::new(text)` | Crea el envolvente con el texto a mostrar. |
| `.with_child(widget)` | Widget que recibirá el efecto hover. |
| `.delay_ms(u32)` | Milisegundos de hover antes de mostrar (por defecto `500`). |
| `.bg_color(Color)` | Color de fondo del panel del tooltip. |
| `.text_color(Color)` | Color del texto. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{Tooltip, Button};

// Botón con tooltip
let btn = Tooltip::<MyApp>::new("Guarda el proyecto (Ctrl+S)")
    .with_child(Box::new(
        Button::new("Guardar").on_click(|ctx| ctx.app.save())
    ))
    .delay_ms(350);

tree.add_node(Box::new(btn), Some(toolbar_id));
```

```rust
// Tooltip instantáneo en un icono de ayuda
let help_icon = Tooltip::<MyApp>::new("Ver documentación completa")
    .with_child(Box::new(Label::new("?")))
    .delay_ms(0);
```

---

## Posicionamiento automático

El tooltip calcula su posición en cada frame: prefiere colocarse **debajo y a la derecha** del cursor. Si el panel no cabe en esa posición, se ajusta:

| Condición | Ajuste |
|-----------|--------|
| Se sale por la derecha | Aparece a la izquierda del cursor |
| Se sale por abajo | Aparece encima del cursor |

El ancho máximo es `240px`; si el texto es más corto, el panel se ajusta.

---

## Anatomía visual

```
      ┌ sombra (offset 2,2, alpha 25%) ─────┐
      ┌─────────────────────────────────────┐
      │ ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔ ← border accent   │
      │  Texto del tooltip                   │
      └─────────────────────────────────────┘
```

- **Sombra**: quad negro al 25% de alpha, desplazado 2px.
- **Panel**: `theme.surface_elevated` o color personalizado, `border-radius: 4px`.
- **Línea superior**: `theme.primary` al 40% — sutilmente identifica como popup.
- **Texto**: `font-size: 12px`, `theme.on_surface` o color personalizado.

---

## Ciclo de vida

```
MouseMove → is_hovered = true
update()  → hover_time_ms += delta_ms
            si hover_time_ms >= delay_ms → is_visible = true → Redraw
MouseLeave → is_hovered = false, is_visible = false, hover_time_ms = 0 → Redraw
```

> [!TIP]
> Para tooltips ricos (con imagen o múltiples líneas), extiende `Tooltip` con
> un widget de contenido personalizado en lugar de usar el texto simple.
