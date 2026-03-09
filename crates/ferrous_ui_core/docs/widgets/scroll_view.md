# ScrollView

`ScrollView` es un contenedor especializado que permite desplazar su contenido cuando este excede las dimensiones visuales del widget. Es el componente fundamental para listas largas, paneles de texto extensos o visualizadores de imágenes grandes.

> **Import** — `ferrous_ui_core::ScrollView`

## Funcionamiento

El `ScrollView` define un "viewport" (ventana de visualización). Los hijos que se añaden dentro del `ScrollView` se dibujan con un desplazamiento (`scroll_offset`) y se recortan automáticamente mediante la pila de `scissor` del renderizador.

## Estructura

```rust
pub struct ScrollView<App> {
    pub scroll_offset: Vec2,
    pub wheel_speed: f32,
    pub is_hovered: bool,
}
```

- `scroll_offset`: Representa cuánto se ha desplazado el contenido en X e Y.
- `wheel_speed`: Multiplicador de velocidad para el desplazamiento con la rueda del ratón.

## Ejemplo de Uso

En la macro `ui!`, el `ScrollView` puede contener cualquier número de hijos:

```rust
ui! {
    ScrollView() {
        Panel() {
            Label("Ítem 1")
            Label("Ítem 2")
            // ... muchos ítems
            Label("Ítem 100")
        }
    }
}
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new()` | Crea un ScrollView con valores por defecto. |
| `with_wheel_speed(f32)` | Ajusta la sensibilidad del ratón (por defecto 20.0). |

## Interacción y Eventos

1. **Mouse Wheel**: Si el cursor está sobre el `ScrollView`, la rueda del ratón actualiza `scroll_offset`.
2. **Clipping**: El `UiTree` aplica automáticamente un `PushClip` con el rect del `ScrollView` antes de dibujar sus hijos y un `PopClip` al terminar.
3. **Culling**: El sistema de renderizado ignora automáticamente los hijos que quedan completamente fuera del área visible para optimizar el rendimiento.

## Notas de Implementación

- Actualmente, el límite de scroll no es automático (se puede scrollear infinitamente hacia abajo/derecha). La limitación basada en el tamaño real del contenido está planeada para la Fase 7.
- No incluye barras de desplazamiento visuales (scrollbars) por defecto; estas deben añadirse como widgets hermanos o superpuestos si se desean.
