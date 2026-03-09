# PlaceholderWidget

`PlaceholderWidget` es el widget más simple del sistema. No tiene estado, no ocupa espacio (por defecto, a menos que el layout diga lo contrario) y no dibuja absolutamente nada.

## Propósito

Se utiliza principalmente para:
- **Uso Estructural:** Mantener un `NodeId` reservado en el árbol que será reemplazado más tarde.
- **Espaciado manual:** Aunque `Spacer` es preferible para layouts flex, a veces se puede usar un nodo vacío para forzar ciertas restricciones.
- **Debugging:** Un lugar donde colgar hijos sin que el contenedor padre interfiera visualmente.

## Estructura

```rust
pub struct PlaceholderWidget;
```

## Implementación

Su implementación del trait `Widget` está vacía, lo que significa que hereda los comportamientos por defecto:
- `draw`: No genera ningún `RenderCommand`.
- `calculate_size`: Devuelve un tamaño de `(0.0, 0.0)`.
- `update`: No realiza ninguna acción.
- `on_event`: Ignora todos los eventos.
