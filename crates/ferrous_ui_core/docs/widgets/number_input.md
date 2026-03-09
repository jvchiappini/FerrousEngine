# NumberInput

`NumberInput` es un widget de entrada especializado que restringe la entrada de caracteres a números y puntos decimales. Es un wrapper conveniente sobre `TextInput` que maneja automáticamente la validación y el parseo de valores numéricos.

## Estructura

```rust
pub struct NumberInput<App> {
    pub inner: TextInput<App>,
}
```

## Características

- **Validación en tiempo real:** Solo permite caracteres numéricos `0-9` y el punto decimal `.`.
- **Parseo automático:** El callback `on_change` recibe el valor ya convertido a `f32`.
- **Basado en TextInput:** Hereda todas las capacidades de renderizado y lógica de `TextInput`.

## Ejemplo de Uso

```rust
use ferrous_ui_core::NumberInput;

let input = NumberInput::new("0.0")
    .on_change(|ctx, value| {
        ctx.app.settings.volume = value;
        println!("Nuevo volumen: {}", value);
    });
```

## Ciclo de Vida

1. **Entrada de caracteres:** El widget filtra los eventos `Char` para asegurar que solo entren números o un punto.
2. **Submit:** Cuando el usuario presiona `Enter`, el texto contenido en el `inner` (TextInput) es parseado como `f32`.
3. **Callback:** Si el parseo es exitoso, se ejecuta el closure registrado en `on_change`.

## Personalización

Dado que es un envoltorio de `TextInput`, utiliza las mismas propiedades de estilo y tema:
- **Fondo:** `theme.surface_elevated`
- **Texto:** `theme.on_surface`
- **Borde (foco):** `theme.primary`
