# 🏁 FerrousEngine: Technical Implementation Checklist

## 1. Low-Level Graphics Abstraction (WGPU Hardware Interface)

* [ ] **Device & Queue Management:** Abstracción de `wgpu::Device` y `wgpu::Queue` para soporte multihilo.
* [ ] **Resource Lifetime Tracking:** Sistema de manejo de ciclos de vida de recursos de GPU (Buffers, Textures) para evitar fugas de memoria de video.
* [ ] **Bind Group Layout Cache:** Implementación de un sistema de hashing para reutilizar `BindGroupLayouts` y evitar redundancia en el pipeline.
* [ ] **Staging Buffer Pool:** Gestión de buffers temporales para transferencias eficientes CPU-to-GPU.
* [ ] **Custom Render Pass Encoder:** Wrapper sobre el encoder nativo para manejo automático de estados de pipeline y Viewport dinámico.

## 2. Off-screen Rendering & Framebuffer Architecture (The RTT Foundation)

* [ ] **Multi-Target Render Pass:** Configuración de `ColorAttachments` múltiples para permitir arquitecturas de renderizado diferido (Deferred Rendering).
 * [x] **Resolution Scaling Logic:** Implementación de texturas de destino desacopladas del tamaño de la ventana física (viewport independiente) para permitir *Super Sampling* o *Dynamic Resolution*.
* [ ] **Depth-Stencil State Management:** Implementación de buffers de profundidad de 32 bits (Z-Buffer) con soporte para *Stencil Masking*.
* [ ] **MSAA Resolver:** Pipeline de resolución de antialiasing multimusestreo antes de pasar la textura al proceso de UI.

## 3. Shader System & Hot-Reloading Pipeline

* [ ] **WGSL Module Compilation:** Abstracción para la creación de `ShaderModules` a partir de archivos externos.
* [ ] **Shader Preprocessing:** Sistema básico de macros (#include, #define) para WGSL.
* [ ] **Filesystem Watcher:** Integración con `notify-rs` para recompilar shaders en tiempo de ejecución sin reiniciar el motor.
* [ ] **Reflection System:** Extracción automática de metadatos de shaders (bindings, locations) para validación en tiempo de ejecución.

## 4. Geometric Data & Memory Layout

 * [x] **Interleaved vs Non-Interleaved Buffers:** Simple vertex struct con atributos interleaved; bytemuck casting funciona.
 * [x] **Unified Mesh Representation:** `Mesh` struct encapsula vértices/índices y permite cubo de prueba. (todavía sin UV/normal)
 * [ ] **AABB (Axis-Aligned Bounding Box) Generation:** Cálculo automático de cajas de colisión para cada malla cargada.
 * [x] **Vertex Format Mapping:** `Vertex::desc()` devuelve layout compatible con pipeline.

## 5. Camera & Spatial Mathematics

 * [ ] **Frustum Culling:** Implementación de algoritmos de descarte de objetos fuera del campo de visión de la cámara.
 * [x] **Camera Controller Abstraction:** `Camera` struct + orbit controller mediante mouse/WASD.
 * [x] **Global Uniform Buffer:** `CameraUniform` enviado a GPU cada frame.
 * [ ] **Coordinate System Normalization:** Gestión de la conversión de coordenadas entre espacio de mundo, espacio de cámara y espacio de clip.

## 6. The Editor Core (UI/UX Integration)

 * [ ] **Egui Paint Callback:** Integración de comandos de dibujo personalizados de `wgpu` dentro del ciclo de renderizado de la UI.
 * [ ] **Texture ID Mapping:** Sistema para convertir `wgpu::TextureView` en `egui::TextureId` de manera dinámica.
 * [x] **Input Propagation System:** Lógica para decidir si un clic pertenece a un widget de la UI o debe ser procesado por la escena 3D (viewport checks & button hover).
 * [ ] **Docking Workspace:** Implementación de un layout persistente con paneles flotantes y anclados (Hierarchy, Inspector, Viewport).

## 7. Entity Component System (Data-Oriented Design)

* [ ] **World & Registry Setup:** Inicialización del contenedor central de entidades.
* [ ] **System Scheduling:** Definición de etapas (Pre-Update, Update, Post-Update, Render).
* [ ] **Component Querying:** Filtrado eficiente de entidades que poseen componentes específicos (ej. `Transform` + `Mesh`).
* [ ] **Parallel System Execution:** Configuración del despachador para ejecutar sistemas en múltiples núcleos cuando no hay dependencias.

## 8. Asset Management & I/O

* [ ] **Async Asset Loading:** Carga de texturas y modelos en hilos secundarios para evitar bloqueos en el hilo principal.
* [ ] **GLTF Scene Parser:** Implementación de un cargador que extraiga no solo la geometría, sino también la jerarquía de nodos y materiales.
* [ ] **Asset Database:** Sistema de indexación basado en UUIDs para referenciar recursos de manera persistente.
* [ ] **Image Decoding Pipeline:** Integración de la crate `image` para soportar formatos industriales (PNG, JPEG, DDS).

## 9. Material & Light Systems (PBR Core)

* [x] **BRDF Implementation:** Sombreadores basados en la función de distribución de microfaccetas (Cook-Torrance).
* [x] **Material Instance System:** Concepto de materiales "padre" y "copias" con diferentes parámetros (como en Unreal).
* [ ] **Point, Directional & Spot Light Components:** Implementación de diferentes tipos de fuentes de luz en el shader.
* [ ] **Shadow Map Atlas:** Gestión de texturas de sombra para múltiples luces en un solo atlas de textura.

## 10. Engine Tools & Debugging

* [ ] **Wireframe Overlay:** Modo de visualización de mallas en modo líneas.
* [x] **Frame Time Profiler:** Visualización gráfica de los milisegundos que tarda cada sistema (CPU vs GPU).
* [x] **Visual Gizmos:** Implementación de ejes de movimiento, rotación y escala interactivos.
* [ ] **Logger Sink:** Redirección de los `println!` y `log` de Rust hacia una consola visual dentro del editor.

## 11. Advanced Material System & PBR (Physically Based Rendering)

* [x] **Metallic-Roughness Workflow:** Implementación estándar de la industria para materiales realistas.
* [x] **Normal Mapping & Tangent Space:** Cálculo de vectores binormales en GPU para detalle superficial.
* [ ] **IBL (Image-Based Lighting):** Uso de mapas de entorno (Equirectangular a Cubemap) para reflexiones realistas y luz difusa ambiental.
* [ ] **Anisotropic Filtering:** Configuración de samplers para evitar distorsión de texturas en ángulos agudos.
* [ ] **Material Graph (Visual Shader Editor):** Backend para un sistema de nodos que genere código WGSL dinámicamente.

## 12. Digital Audio Subsystem (The Sound Engine)

* [ ] **Audio Device Abstraction:** Integración con `cpal` o `rodio` para salida de audio multiplataforma.
* [ ] **Spatial 3D Audio:** Implementación de atenuación por distancia y paneo basado en la posición de la entidad `Listener`.
* [ ] **Audio Bus Routing:** Creación de un mixer con canales (Master, SFX, Music) y efectos (Reverb, Low-pass).
* [ ] **Real-time FFT Analysis:** Sistema para extraer datos de frecuencia y usarlos en visualizaciones o gameplay.
* [ ] **Vorbis/WAV Streaming:** Carga por demanda de archivos largos para evitar picos de memoria RAM.

## 13. High-Performance UI Framework (The Editor & Game UI)

* [x] **Retained vs Immediate Mode Hybrid:** Uso de `egui` para el editor y un sistema de UI retenido (nodos) para el juego final.
* [ ] **SDF Text Rendering (Signed Distance Fields):** Renderizado de fuentes basado en vectores para mantener nitidez en cualquier resolución.
* [x] **Flexbox/Grid Layout Engine:** Implementación de algoritmos de posicionamiento automático de elementos de interfaz.
* [ ] **Event Bubbling System:** Propagación de eventos de clic y scroll a través de la jerarquía de la UI.
* [ ] **Skinning & Theming:** Desacoplamiento de la lógica de UI de la apariencia visual mediante hojas de estilo.

## 14. Physics & Collision Detection

* [ ] **Physics Engine Integration:** Binding con `Rapier3d` (el motor físico líder en Rust).
* [ ] **Rigid Body Dynamics:** Soporte para cuerpos estáticos, dinámicos y cinemáticos.
* [ ] **Collision Callbacks:** Sistema de eventos `on_collision_enter` integrados con el ECS.
* [ ] **Raycasting & Shape Casting:** Consultas espaciales para detección de suelo, línea de visión y puntería.
* [ ] **Character Controller:** Lógica especializada para el movimiento de jugadores (evitar traspasar paredes, subir escalones).

## 15. Animation Pipeline

* [ ] **Skeletal Animation (Skinning):** Deformación de mallas basada en un esqueleto de huesos mediante Vertex Skinning en GPU.
* [ ] **Animation Blending:** Transición suave entre estados (ej. caminar a correr) mediante interpolación lineal.
* [ ] **Inverse Kinematics (IK):** Ajuste procedimental de extremidades (pies que se adaptan al terreno).
* [ ] **Animation State Machine:** Grafo de estados para gestionar la lógica de animaciones del personaje.

## 16. Scene Management & Persistence

* [ ] **Prefab System:** Capacidad de crear plantillas de entidades reutilizables con hijos y componentes preconfigurados.
* [ ] **Scene Serialization:** Guardado de la jerarquía completa del ECS a formato binario (Bincode) o legible (RON/YAML).
* [ ] **Dynamic Level Loading:** Sistema de "streaming" de celdas del mundo para evitar pantallas de carga largas.
* [ ] **Undo/Redo Command Pattern:** Historial de acciones en el editor para revertir cambios en la escena.

## 17. Multithreading & Optimization (The Rust Advantage)

* [ ] **Work-Stealing Job System:** Reparto de tareas pesadas (física, culling, IA) entre todos los núcleos disponibles.
* [ ] **GPU Driven Rendering:** Mover el frustum culling y el LOD (Level of Detail) completamente a Compute Shaders.
* [ ] **Memory Pool Allocators:** Uso de arenas de memoria para componentes del ECS para mejorar el caché de la CPU.
* [ ] **Occlusion Culling:** No renderizar objetos que están totalmente tapados por otros más grandes.

## 18. Scripting & Logic Extension

* [ ] **Scripting Host:** Integración de un lenguaje embebido como **Rhai** o **Lua** (vía `mlua`) para lógica de alto nivel.
* [ ] **Hot-Reloading Logic:** Posibilidad de cambiar scripts y ver los cambios en el juego sin recompilar el motor en Rust.
* [ ] **Native Plugins:** Sistema de carga de librerías dinámicas (`.dll` / `.so`) para extender el motor.

## 19. Build & Deployment Tools

* [ ] **Asset Cooker:** Compresión y optimización de texturas y modelos para el "build" final de producción.
* [ ] **Cross-Compilation Pipeline:** Scripts para compilar desde Windows hacia Linux, MacOS y WebAssembly (WASM).
* [ ] **Shader Stripping:** Eliminar variantes de shaders no utilizadas para reducir el tamaño del ejecutable.

## 20. Post-Processing Stack

* [ ] **Tone Mapping:** Conversión de HDR a SDR usando algoritmos como ACES o Reinhard.
* [ ] **Bloom & Glow:** Efecto de sangrado de luz para objetos brillantes.
* [ ] **SSAO (Screen Space Ambient Occlusion):** Sombras de contacto suaves en esquinas y grietas.
* [ ] **Temporal Anti-Aliasing (TAA):** Reducción de bordes dentados usando información de frames anteriores.

