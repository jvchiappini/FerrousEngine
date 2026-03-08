# ToggleSwitch

`ToggleSwitch` es una alternativa visual al `Checkbox`, diseñada con un estilo deslizante (tipo mobile/tablet), ideal para configuraciones y preferencias.

## Características

- **Genérico sobre App:** Permite mutar el estado de la aplicación mediante callbacks de evento.
- **Animado (Conceptualmente):** En el modo retenido, facilita la implementación de animaciones de desplazamiento suave entre estados.

## Ejemplo de Uso

```rust
let sw = ToggleSwitch::new(false)
    .on_toggle(|ctx, enabled| {
        ctx.app.settings.notifications = enabled;
    });
```

## Estilo

- **Track (Fondo):** `theme.surface_elevated`.
- **Thumb (Círculo):** `theme.on_primary` (o blanco).
- **Estado Activo:** El track cambia a `theme.success` o `theme.primary`.
