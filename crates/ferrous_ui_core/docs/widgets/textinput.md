# TextInput

`TextInput` es un campo de texto editable de una sola línea. Soporta el foco del teclado, movimiento del cursor y selección (Phase 4.0).

## Características

- **Cursor Dinámico:** Visualiza la posición de inserción.
- **Data Binding:** Puede vincularse a un `Observable<String>` para actualizaciones bidireccionales automáticas.
- **Scroll Interno:** Soporta texto más largo que el ancho del widget.

## Estructura

```rust
pub struct TextInput<App> {
    pub text: String,
    pub cursor_pos: usize,
    pub placeholder: String,
    pub is_password: bool,
    // Callbacks y binding...
}
```

## Ejemplo de Uso

```rust
let input = TextInput::new("Valor inicial")
    .with_placeholder("Escribe aquí...")
    .on_change(|ctx, new_text| {
        println!("Editando: {}", new_text);
    });
```

## Estilo

Utiliza los siguientes roles del `Theme`:
- **Fondo:** `theme.background` (o una variante del `surface`).
- **Texto:** `theme.on_surface`.
- **Cursor:** `theme.primary`.
- **Placeholder:** `theme.on_surface_muted`.
