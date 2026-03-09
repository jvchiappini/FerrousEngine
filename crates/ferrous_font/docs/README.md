# ferrous_font

`ferrous_font` es el sistema de gestión de fuentes de FerrousEngine. Se encarga de parsear archivos TrueType/OTF, generar bitmaps de glifos mediante **MSDF** (Multi-channel Signed Distance Fields) y empaquetarlos en un atlas de texturas para la GPU.

## Características

- **MSDF Rendering:** Permite que el texto se vea nítido en cualquier nivel de zoom sin pixelación.
- **Atlas Dinámico:** Genera una textura única que contiene todos los glifos necesarios para la sesión actual.
- **Independiente de la Plataforma:** Funciona nativamente en Desktop y mediante fallbacks en WASM.
- **Parser Nativo:** Implementación propia de lectura de tablas TrueType (`cmap`, `glyf`, `head`, `hhea`, `hmtx`, `loca`).

## Module Overview

| Módulo | Función |
|--------|---------|
| `parser` | Lee y decodifica la estructura binaria de los archivos `.ttf` y `.otf`. |
| `msdf_gen` | Genera las distancias con signo para cada glifo, permitiendo bordes suaves. |
| `atlas` | Gestiona el empaquetado de glifos en una textura (bin-packing) y su subida a la GPU. |
| `lib` | Expone la estructura `Font`, el punto de entrada principal para cargar tipografías. |

## Ejemplo de Uso

```rust
use ferrous_font::Font;

// Cargar una fuente desde el sistema de archivos
let font = Font::load(
    "assets/fonts/Inter-Regular.ttf",
    &device,
    &queue,
    "abcdefghijklmnopqrstuvwxyz0123456789".chars()
);

// El objeto font.atlas ahora contiene la textura lista para usar en RenderCommands
```

## Detalles Técnicos

### MSDF (Multi-channel Signed Distance Fields)
A diferencia de los mapas de bits tradicionales, MSDF almacena la distancia a los bordes del glifo en tres canales de color. Esto permite reconstruir la forma del carácter en el fragment shader con una calidad superior a los SDF estándar (monoculares), preservando mejor las esquinas afiladas.

### Fallback Font
Si la carga de una fuente falla o la aplicación se ejecuta en un entorno sin acceso a archivos (como la web sin pre-fetch), el crate incluye una fuente minimalista integrada en binario que contiene solo los caracteres esenciales para evitar que la aplicación falle.

---

## Further Reading
- [Backend de renderizado — ferrous_ui_render](../../ferrous_ui_render/docs/README.md)
- [Sistema de assets — ferrous_assets](../../ferrous_assets/docs/README.md)
