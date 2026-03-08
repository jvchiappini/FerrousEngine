# ferrous_ui_render

`ferrous_ui_render` es el backend de dibujo optimizado para GPU. Su función es tomar los `RenderCommand` abstractos generados por el sistema de UI y traducirlos en operaciones eficientes de **WGPU**.

---

## Estrategia de Renderizado: Batching Masivo

El renderer está diseñado para enviar el máximo número posible de primitivas en una sola llamada de dibujo (Draw Call).

- **`GuiBatch`:** Agrupa rectángulos (`Quads`) e imágenes. Puede manejar hasta 8 texturas simultáneas mediante arrays de texturas en el shader.
- **`TextBatch`:** Especializado en el dibujado de texto masivo utilizando atlas de fuentes (Font Atlas).

---

## El Proceso de Traducción (`ToBatches`)

El trait `ToBatches` es el puente entre el crate de lógica (`ferrous_ui_core`) y este backend. Convierte comandos abstractos en datos listos para la memoria de la GPU (`bytemuck::Pod`):

- **Quad:** Rectángulos sólidos con soporte nativo para **bordes redondeados** calculados en el shader.
- **Image:** Rectángulos texturizados.
- **Text:** Generación de geometría basada en glifos de fuentes.

---

## Componentes Técnicos

| Componente | Responsabilidad |
|------------|-----------------|
| `GuiRenderer` | Gestiona los `WGPUPipelines`, los buffers de instancia y el estado global de la GPU. |
| `GuiQuad` | Estructura `repr(C)` que representa un rectángulo en la GPU (80 bytes por instancia). |
| `TextQuad` | Estructura optimizada para glifos de texto. |

---

## Shaders (WGSL)

El renderer utiliza shaders personalizados ubicados en `assets/shaders/`:
- `gui.wgsl`: Maneja la geometría de quads, bordes redondeados y muestreo de texturas.
- `text.wgsl`: Optimizado para la rasterización de texto desde un atlas.

---

## Requisitos de Capacidad

El `GuiRenderer` es dinámico; si el número de elementos en pantalla supera la capacidad actual del buffer de la GPU, el renderer redimensionará automáticamente sus buffers de instancia para alojar la nueva carga sin intervención del usuario.
