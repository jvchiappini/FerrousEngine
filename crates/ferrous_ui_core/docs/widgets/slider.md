# Slider

`Slider` es un widget de entrada que permite al usuario seleccionar un valor dentro de un rango determinado arrastrando un control deslizante.

> **Import** — `ferrous_ui_core::Slider`

## Campos y Configuración

```rust
pub struct Slider<App> {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub is_dragging: bool,
    pub binding: Option<Arc<Observable<f32>>>,
}
```

- `value`: El valor actual del slider (si no hay binding).
- `min` / `max`: Límites del rango.
- `binding`: Vinculación reactiva opcional para control sincronizado.

## Ejemplo de Uso

```rust
use ferrous_ui_core::Slider;

// Slider básico con callback
let s1 = Slider::new(0.5, 0.0, 1.0)
    .on_change(|ctx, val| {
        println!("Volumen: {:.2}", val);
    });

// Slider con Binding Reactivo
let vol_obs = Arc::new(Observable::new(0.7));
let s2 = Slider::new(0.0, 0.0, 1.0)
    .with_binding(vol_obs.clone(), node_id);
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(val, min, max)` | Crea un slider con valor inicial y rango. |
| `on_change(closure)` | Callback que se dispara cada vez que cambia el valor. |
| `with_binding(obs, id)`| Vincula el valor a un `Observable<f32>`. |

## Anatomía Visual

El slider se dibuja usando tres capas:
1. **Track (Pista):** Fondo horizontal sutil (`theme.on_surface_muted`).
2. **Fill (Llenado):** Barra de progreso que indica el valor actual (`theme.primary`).
3. **Knob (Pomo):** Círculo arrastrable que marca la posición (`theme.on_primary`).

## Interacción

- **Click e Inicio de Arrastre:** En `MouseDown`, se calcula la nueva posición y se activa el flag `is_dragging`.
- **Arrastre:** Mientras `is_dragging` es true, cualquier `MouseMove` actualiza el valor proporcionalmente.
- **Fin de Arrastre:** `MouseUp` desactiva el arrastre.

> [!TIP]
> El sistema de "Lag Cero" asegura que el movimiento del pomo sea suave incluso en escenas con miles de nodos, ya que solo el slider y sus comandos directos se invalidan durante el arrastre.
