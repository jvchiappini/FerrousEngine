# 🏗️ Ferrous UI Core

`ferrous_ui_core` es el cerebro del sistema de interfaz de usuario de FerrousEngine. Implementa un modelo de **Modo Retenido** (Retained Mode) diseñado para el alto rendimiento y el "Lag Cero".

## 🛡️ Estructuras Fundamentales

### 1. `UiTree`
El gestor principal del árbol de widgets. 
- Utiliza un `SlotMap` para almacenar los `Node`s.
- Proporciona estabilidad de IDs (`NodeId`). Incluso si los widgets cambian de posición en la memoria, su ID se mantiene constante.
- Gestiona la jerarquía (padre/hijos).

### 2. `Node`
La unidad de almacenamiento en el árbol. Contiene:
- `widget`: Boxed dyn Widget.
- `parent` / `children`: Enlaces jerárquicos.
- `style`: Preferencias de diseño (Padding, Margin, Alignment).
- `rect`: Geometría final calculada por el motor de layout.
- `cached_cmds`: Caché de comandos de dibujo para evitar la re-generación en cada frame.
- `dirty`: Flags de estado (`layout`, `paint`, `hierarchy`).

### 3. Trait `Widget`
Cualquier componente UI de Ferrous debe implementar este trait:
- `build()`: Se llama al insertar el widget. Es el lugar para añadir hijos.
- `update()`: Lógica por frame.
- `draw()`: Genera la lista de `RenderCommand`.
- `on_event()`: Recibe e interactúa con eventos del usuario.

## 🚀 Optimización: Dirty Flags
El sistema utiliza una propagación de flags `subtree_dirty`. Si un nodo en la profundidad del árbol cambia, solo se marcan sus padres como sucios hacia arriba. Durante el recorrido de renderizado, si un nodo tiene `subtree_dirty = false`, se salta toda su descendencia instantáneamente.

## 📦 Widgets Disponibles
- `Panel`: Contenedor visual básico.
- `Label`: Visualización de texto.
- `Button`: Botón interactivo con estados.
- `PlaceholderWidget`: Fallback estructural.
