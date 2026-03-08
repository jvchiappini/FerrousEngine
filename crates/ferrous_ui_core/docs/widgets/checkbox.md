# Checkbox

`Checkbox` es un control binario (Toggle) para valores booleanos. Permite activar o desactivar opciones y puede estar vinculado a un `Observable<bool>`.

## Características

- **Interactividad:** Cambia de estado al hacer clic.
- **Data Binding:** Soporta `Observable<bool>` para actualizaciones reactivas bidireccionales.
- **Diseño Tematizado:** Su apariencia se adapta automáticamente al `Theme` activo.

## Estructura

```rust
pub struct Checkbox<App> {
    pub checked: bool,
    pub binding: Option<Arc<Observable<bool>>>,
    // on_toggle callback...
}
```

## Ejemplo de Uso

```rust
let cb = Checkbox::new(true)
    .on_toggle(|ctx, is_checked| {
        println!("VSync: {}", is_checked);
    });
```

## Estilo

- **Marco:** Utiliza `theme.on_surface_muted` para el borde en estado inactivo.
- **Fondo Activo:** Utiliza `theme.primary` cuando está marcado.
- **Marca (Check):** Utiliza `theme.on_primary`.
