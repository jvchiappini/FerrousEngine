# Slider

`Slider` es un control deslizante para valores numéricos (`f32`). Soporta tanto el manejo manual mediante callbacks como la vinculación automática a través del sistema reactivo (`Observable<f32>`).

## Características

- **Genérico sobre App:** Al igual que `Button`, permite acceder al estado de la aplicación en su callback `on_change`.
- **Modo Reactivo:** Puede vincularse a un `Observable<f32>`, lo que permite que el Slider se actualice solo cuando el valor cambia externamente, y viceversa.
- **Lag Cero:** Solo se redibuja cuando el valor cambia o hay interacción.

## Estructura

```rust
pub struct Slider<App> {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub is_dragging: bool,
    pub binding: Option<Arc<Observable<f32>>>,
}
```

## Uso con Callback

```rust
let slider = Slider::new(0.5, 0.0, 1.0)
    .on_change(|ctx, new_val| {
        println!("Valor: {:.2}", new_val);
        // ctx.app.volume = new_val;
    });
```

## Uso Reactivo (Data Binding)

```rust
let volume_obs = Arc::new(Observable::new(0.5));

// El slider se suscribe al observable automáticamente
let slider = Slider::new(0.0, 0.0, 1.0)
    .with_binding(volume_obs.clone(), node_id);
```

## Estilo

El Slider utiliza el `Theme` para su apariencia:
- **Track (Fondo):** `on_surface_muted` con opacidad reducida.
- **Fill (Progreso):** `primary`.
- **Knob (Círculo):** `on_primary`.
