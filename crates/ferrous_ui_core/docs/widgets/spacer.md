# Spacer

`Spacer` es un widget invisible de utilidad técnica que se utiliza para crear huecos dinámicos entre otros widgets dentro de layouts flexibles (`FlexRow`, `FlexColumn`).

> **Import** — `ferrous_ui_core::Spacer`

## Propósito

A diferencia de un `Margin` o `Padding` que son estáticos, el `Spacer` está diseñado para expandirse y llenar el espacio disponible cuando se combina con estilos de crecimiento (`flex-grow: 1.0`).

## Ejemplo de Uso

Un caso de uso común es empujar elementos a los extremos de una barra de herramientas:

```rust
ui! {
    FlexRow() {
        Button("Archivo")
        Button("Edición")
        Spacer() // Empuja el siguiente botón a la derecha
        Button("Cerrar")
    }
}
```

## Estructura

Es un struct vacío (`Zero Sized Type`) ya que no posee estado ni genera comandos de renderizado:

```rust
pub struct Spacer;
```

## Detalles Técnicos

- **Dibujo**: No genera ninguna instrucción para la GPU (`draw` es un NOP). 
- **Layout**: Reporta un tamaño intrínseco de `0x0`. Su comportamiento de expansión depende enteramente de las propiedades de Flexbox aplicadas al nodo que lo contiene.
