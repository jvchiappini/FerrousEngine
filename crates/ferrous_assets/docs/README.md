# ferrous_assets

`ferrous_assets` es el sistema de gestión de recursos de FerrousEngine. Proporciona una infraestructura para cargar, cachear y acceder de forma eficiente a texturas, modelos, sonidos y otros datos en tiempo de ejecución.

## Características

- **Carga Asíncrona:** Preparado para cargar recursos en segundo plano sin bloquear el hilo principal de renderizado.
- **Sistema de Handles:** Utiliza identificadores ligeros para referenciar recursos, permitiendo la compartición eficiente en memoria.
- **Hot-Reloading (Fase 7):** Integración planeada con el sistema de archivos para recargar assets automáticamente al ser modificados.
- **Extensibilidad:** Soporta diversos formatos mediante un sistema de cargadores (Loaders) modulares.

## Conceptos Clave

### `AssetServer`
El gestor central que coordina las peticiones de carga y el ciclo de vida de los recursos.

### `Handle<T>`
Una referencia tipada a un asset. Es ligero (como un puntero inteligente) y permite que múltiples sistemas usen el mismo recurso sin duplicar datos en memoria.

## Roadmap

- [ ] **Serialización Uniforme:** Integración total con el formato `.fui` para el Ferrous Builder.
- [ ] **Streaming de Texturas:** Cargar versiones de baja resolución primero y mejorar la calidad dinámicamente.
- [ ] **Empaquetado (Bundling):** Herramienta para combinar miles de archivos pequeños en archivos .pak optimizados para producción.

---

## Further Reading
- [Arquitectura de UI — ferrous_ui_core](../../ferrous_ui_core/docs/README.md)
- [Tipos de assets — ferrous_asset_types](../ferrous_asset_types/README.md)
