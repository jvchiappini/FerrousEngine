# 🏗️ Ferrous GUI: Arquitectura Maestra

Ferrous GUI es el sistema definitivo de interfaz de usuario de FerrousEngine, diseñado para aplicaciones de alta carga gráfica (como editores 3D y herramientas de producción) bajo la filosofía de **"Lag Cero"**.

## 🧩 Estructura Modular

La librería está fragmentada en cuatro pilares fundamentales para garantizar la mantenibilidad y el máximo rendimiento:

### 1. [Core (Cerebro)](../../ferrous_ui_core/docs/CORE.md)
Implementa el **Modo Retenido** (Retained Mode).
- Árbol de widgets persistente (`UiTree`).
- Ciclo de vida y Dirty Flags.
- Definición abstracta de comandos de dibujo (`RenderCommand`).

### 2. [Layout (Geometría)](../../ferrous_layout/docs/LAYOUT.md)
Calcula posiciones y dimensiones.
- Integración con el motor **Taffy** de alto rendimiento.
- Soporte para **Flexbox** y Grid.
- Coordenadas absolutas resueltas a partir de restricciones relativas.

### 3. [Events (Interacción)](../../ferrous_events/docs/EVENTS.md)
Maneja la intención del usuario.
- Hit-testing recursivo y eficiente.
- Bubbling de eventos del hijo al padre.
- Estados automáticos de Hover y Focus.

### 4. [Render (Visualización)](../../ferrous_ui_render/docs/RENDER.md)
Backend gráfico de bajo nivel.
- Traduce el árbol abstracto a la GPU mediante **WGPU**.
- Batching masivo de quads y glifos de texto.
- Shaders especializados para bordes redondeados y efectos visuales.

---

## 🔄 El Pipeline de un Frame

1.  **Entrada**: Recibe eventos del OS (vía `winit`).
2.  **Despacho**: `EventManager` localiza el widget objetivo y burbujea el evento.
3.  **Lógica**: Se ejecuta `widget.update()` para animaciones y cambios de estado.
4.  **Layout**: Si hay cambios estructurales, el `LayoutEngine` recalcula el árbol.
5.  **Generación**: Los widgets "sucios" regeneran sus `RenderCommand`.
6.  **Dibujo**: El `GuiRenderer` envía los lotes optimizados a la GPU.

---

Este enfoque desacoplado permite que Ferrous GUI escale hasta **cientos de miles de nodos** sin penalizar la tasa de frames, manteniendo una experiencia de usuario fluida y reactiva.
