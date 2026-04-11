# FerrousEngine: The Next-Gen Global Engine

**FerrousEngine** no es simplemente un motor de renderizado superficial o un empaquetado; es un *framework arquitectónico global ultraligero* de nivel industrial, construido en Rust (con el ecosistema estándar moderno de WGPU) e ideado de raíz para ser la fundación definitiva y absoluta de cualquier aplicación gráfica seria y potente.

Ha sido planeado para escalar infinitamente y servir como núcleo transversal tanto para **Videojuegos Triple-A** interactivos, como para **Aplicativos Desktop Híbridos (Tooling / CAD)** y sorprendentes **Aplicaciones Web Interactivas** que opaquen a otras librerías web veteranas como *Three.js*.

## 🚀 Filosofía Principal: 1000% Escalable, 100% Configurable

Tomando inspiración en el poder masivo y subyacente del Core de C++ de *Unreal Engine*, y en la facilidad de consumo y agilidad web de *Three.js*, FerrousEngine divide las responsabilidades con precisión quirúrgica:

A través de un puente bidireccional (vía WebAssembly Bindings o FFI), **tú dictas comandos (Command Queues)** desde tu idioma preferido (JavaScript/TypeScript, C++) desde los estratos de de tu interfaz de aplicación (React, Vue, Tauri, Winit). Mientras tanto, el abrumador poder en Rust maneja en background tu memoria y VRAM, calculando jerarquías matemáticas del Entity-Component-System (ECS) e imprimiendo los fotogramas sin bloquear jamás tu flujo productivo original a 60-144 fotogramas por segundo consecuentes.

---

## 🛠 Características de Clase Mundial

### 1. Despliegue Universal Auténtico (Write Once, Deploy Everywhere)
*   **Desktop App Development (Nativo):** Operando directamente con Vulkan, Metal y DirectX12 ofrece renderizado nativo sin el terrible overhead de un navegador, perfecto para la creación de Editores de software y simulaciones industriales.
*   **Embeddable Web-Oriented Crate (`ferrous_web`):** Agnóstico de archivos y del DOM de React/Vue. Gracias a nuestra implementación pura mediante `web_sys` y `wasm-bindgen-futures`, la carga de pesadas mallas `.glb` o Texturas PBR 4K se descarga del Servidor empleando asincronía extrema que carga progresivamente sin estancar nunca el event loop web; tu página HTML no pierde responsive al encender la capa inmersiva WebGPU.

### 2. Render Pipeline PBR Altamente Completo 
*   No necesitas armar luces y primitivas experimentales desde ceros. Nuestro Pass Rendering incluye:
    *   **ACES Tonemapping (HDR Pipeline)** de rango dinámico para que la óptica sea hiper-realista.
    *   Iluminación Base PBR Global con materiales Avanzados (Roughness/Metallic, **Clearcoat**, **Opacity**).
    *   Arquitectura Data-Oriented (ECS) preparada por defecto para **Instancing Masivo** (dibujar miles de assets copiados gastando solo un Draw Call en CPU).
    *   Soporte dinámico opcional en la estructura modular para Voxel Global Illumination, SSGI y WebXR Opcional.

### 3. Organización Anti-Monolítica (Sin God Files)
Tu proyecto jamás pesará o incluirá módulos que te estorben. Ferrous separa sus librerías a lo largo de Crates minúsculas, seguras, limpias y altamente tipadas:
*   `ferrous_core`: Todas las matemáticas, Transforms y el Entity-Component-System general.
*   `ferrous_renderer`: El cerebro oscuro WebGPU / WGPU que ensambla el Render Graph.
*   `ferrous_ecs`: Estructuras de almacenamiento cache-friendly velocísimas basadas en componentes y Data-Oriented Design.
*   `ferrous_assets`: I/O e Ingesta. 
*   `ferrous_gui`: Si requirieses crear UI directamente sobre el Contexto Hardware y evitar usar CSS temporalmente o para herramientas de escritorio internas.

### 4. Zero-Friction Interop
Expón una instancia global *Singleton* hacia cualquier Front-End mediante nuestros bindings fluentes:
```javascript
import { FerrousWebEngine } from 'ferrous_web';

// Instanciar el Core Industrial (Rust)
const engine = new FerrousWebEngine();

// Arrancarlo y montarlo de fondo 
engine.mount_and_run(); 

// API Fluente: Crea y configura objetos en una sola línea
engine.createBox("HeroBox", 0, 0, 0, 1, 1, 1, 1, 0, 0)
      .set_position(0, 5, 0)
      .set_material(1, 0.5, 0, 0.8, 0.1); // Color, Metal, Roughness

// Control atmosférico dinámico
engine.set_environment(0.1, 0.1, 0.15, 0.02); // Fog color & density
engine.set_exposure(0.6); // HDR Exposure
```

---

*Desarrolla Software para el presente de manera impecable; renderízalo para el futuro sin límites técnicos ni parches arquitectónicos de rendimiento.*
