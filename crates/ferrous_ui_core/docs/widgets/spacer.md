# Spacer

`Spacer` es un widget estructural invisible que se expande para llenar el espacio restante en un contenedor Flex. Es análogo al `Spacer()` de SwiftUI.

## Características

- **Sin Dibujo:** No genera ningún `RenderCommand`.
- **Efecto de Layout:** Se utiliza para separar widgets (ej: empujar un botón al final de una fila).

## Ejemplo de Uso

```rust
// Fila con un botón al principio y otro al final
ui! {
    HStack {
        Button("Inicio")
        Spacer()
        Button("Fin")
    }
}
```

## Mecanismo

`Spacer` devuelve un factor de `flex(1.0)` por defecto en su estilo, lo que le permite absorber el espacio sobrante en el eje principal del contenedor Flex.
