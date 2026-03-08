# 🎨 Ferrous UI Render

`ferrous_ui_render` es el backend de renderizado GPU para la UI de FerrousEngine basado en **WGPU**. 

## 🛡️ Núcleo Gráfico
El renderizador está diseñado para minimizar drásticamente las llamadas de dibujo (Draw Calls) mediante el agrupamiento masivo de primitivas.

### 1. `GuiBatch` (Lote de Quads)
Agrupa cientos de rectángulos en un solo lote. Cada `GuiQuad` se define en memoria para un procesamiento masivo en la GPU:
- Posición, tamaño y radios de bordes redondeados.
- Color (RGBA) y coordenadas UV.
- `tex_index`: Índice de la textura dentro del lote actual (máximo 8 texturas por lote).
- `flags`: Bits de configuración para el shader (ej. texturizado, degradado).

### 2. `TextBatch` (Lote de Texto)
Optimizado especialmente para el dibujado de glifos.
- Utiliza una técnica de rasterización desde un atlas de fuentes (`FontAtlas`).
- Cada glifo es un `TextQuad` que referencia las coordenadas UV correctas del atlas.

### 3. `GuiRenderer`
El motor principal de renderizado.
- Gestiona pipelines de WGPU para quads generales y para texto.
- Implementa buffers de instancia dinámicos que se redimensionan automáticamente según la cantidad de widgets en pantalla.
- Soporta renderizado mantenido (`render`) y con limpieza de buffer (`render_clearing`).

## 🚀 Traducción: `ToBatches`
El trait `ToBatches` actúa como el puente final:
- Toma los `RenderCommand` abstractos generados por los widgets en `ferrous_ui_core`.
- Traduce los comandos a primitivas del renderizador (`GuiQuad`, `TextQuad`).
- Organiza los comandos en los lotes adecuados para maximizar el paralelismo en la GPU.

## 💎 Características Avanzadas
- **Bordes Redondeados**: Implementados eficientemente en el shader para cualquier radio de esquina de forma independiente.
- **Batching de Texturas**: Soporta hasta 8 texturas simultáneas por lote de dibujo sin cambiar el estado del pipeline.
- **Multiplicidad de Instancias**: Soporta miles de nodos renderizados en un solo frame con un coste de CPU mínimo.
