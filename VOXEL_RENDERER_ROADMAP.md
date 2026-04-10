# FerrousEngine — Voxel Renderer Roadmap
<!-- 
  ARCHIVO DE CONTEXTO PERSISTENTE
  Leer SIEMPRE al inicio de cada sesión antes de implementar cualquier fase.
  Contiene el estado actual, las fases completadas y los detalles técnicos exactos.
-->

## Objetivo

Añadir a FerrousEngine un renderer de voxels con GI completamente dinámica que compita
visualmente con path tracing completo, corriendo a 60fps en RTX 3070 con DLSS, con
soporte de destrucción dinámica y sin stutter.

El renderer se implementa como un **nuevo crate** (`crates/ferrous_voxels`) que se
integra en `ferrous_renderer` usando el sistema de `RenderPass` + `ComputePass`
ya existente. NO se modifica el pipeline rasterizado actual.

---

## Estado actual del engine (contexto base)

```
ferrous_render_graph    → RenderPass trait (prepare + execute), FramePacket
ferrous_renderer        → Renderer, extra_passes: Vec<Box<dyn RenderPass>>,
                          ComputePass, ComputePipeline, PostProcessPass,
                          frustum culling (GPU-driven), SSAO, PBR, CelShading
ferrous_ecs             → ECS con query por componente
ferrous_core            → World, Scene, Camera3D, DirectionalLight, Frustum
ferrous_app             → Runner, AppContext, AppMode (Game3D, Desktop2D, etc.)
ferrous_gpu             → EngineContext (device, queue, surface)
```

**Lo que YA existe y se reutiliza:**
- `RenderPass` trait con `prepare` / `execute` / `on_resize`
- `ComputePass` + `ComputePipeline` (wgsl compute dispatch en el render graph)
- `extra_passes: Vec<Box<dyn RenderPass>>` para enchufar passes custom
- `Frustum::from_view_proj` para culling
- HDR pipeline (`Rgba16Float`) → ACES tonemap en `post.wgsl`
- `wgpu 23.0`, `glam 0.29`

---

## Arquitectura del nuevo crate: `ferrous_voxels`

```
crates/ferrous_voxels/
├── Cargo.toml
└── src/
    ├── lib.rs                  → API pública, VoxelRenderer struct
    ├── dag/
    │   ├── mod.rs
    │   ├── hash_dag.rs         → HashDAG CPU (HashMap<u64, DAGNode>, BitGrid)
    │   ├── node.rs             → DAGNode, Voxel struct
    │   └── gpu_upload.rs       → Sync HashDAG → GPUBuffer
    ├── world/
    │   ├── mod.rs
    │   ├── chunk_manager.rs    → Chunk streaming, dirty tracking
    │   └── voxel_edit.rs       → API pública de edición (set_voxel, destroy, build)
    ├── passes/
    │   ├── mod.rs
    │   ├── hdda_pass.rs        → ComputePass: HDDA primary raymarching + G-Buffer
    │   ├── restir_pass.rs      → ComputePass: ReSTIR DI (candidates + visibility)
    │   ├── gi_pass.rs          → ComputePass: SSRC + DDGI lookup + WSRC
    │   ├── svgf_pass.rs        → ComputePass: denoiser temporal + wavelet
    │   └── taa_pass.rs         → ComputePass/RenderPass: TAA + composite final
    ├── lighting/
    │   ├── mod.rs
    │   ├── light_bvh.rs        → Light BVH (sol + puntuales + emisivos)
    │   └── ddgi.rs             → DDGIProbeGrid, probe update budget
    ├── cache/
    │   ├── mod.rs
    │   └── wsrc.rs             → World Space Radiance Cache (HashMap espacial GPU)
    ├── buffers/
    │   ├── mod.rs
    │   └── persistent.rs       → PersistentBuffers (history, confidence, motion)
    └── shaders/                → (assets/shaders/voxels/ en el workspace root)
        (los .wgsl viven en assets/shaders/voxels/)
```

**Shaders WGSL a crear** (en `assets/shaders/voxels/`):
```
hdda_primary.wgsl       → HDDA two-level + G-Buffer output + fog inline
hdda_shadow.wgsl        → Shadow ray binario (para ReSTIR)
restir_candidates.wgsl  → Generar 7 candidatos por pixel
restir_visibility.wgsl  → Evaluar shadow ray del ganador
restir_reuse.wgsl       → Temporal + spatial reuse de reservoirs
gi_ssrc.wgsl            → Screen-space radiance cache (2 bounce rays)
gi_ddgi_update.wgsl     → Actualizar probes (32 rayos/probe)
gi_ddgi_sample.wgsl     → Interpolación de 8 probes por pixel
gi_wsrc.wgsl            → World space radiance cache lookup/compute
svgf_temporal.wgsl      → Acumulación temporal (confidence-weighted)
svgf_wavelet.wgsl       → Filtro à-trous (5 iteraciones)
svgf_composite.wgsl     → Reconstrucción final (albedo * di + gi + emissive + fog)
taa.wgsl                → TAA + jitter Halton
```

---

## GPU Buffers persistentes entre frames

```rust
// En PersistentBuffers (buffers/persistent.rs):
gbuffer_current:    Texture2D<Rgba32Float>   // worldPos + normalID + albedo + ...
gbuffer_history:    Texture2D<Rgba32Float>   // frame anterior (double buffer)
radiance_history:   Texture2D<Rgba16Float>   // 3 canales: DI_diff, DI_spec, GI
confidence:         Texture2D<R32Float>      // frames de historia válida
motion_vectors:     Texture2D<Rg16Float>     // screen-space motion
invalidation_mask:  Texture2D<R8Uint>        // pixels que perdieron historia
reservoirs:         StorageBuffer            // ReSTIR reservoirs (current + history)
dag_nodes:          StorageBuffer            // HashDAG GPU-side
dag_occupancy:      StorageBuffer            // BitGrid por nivel
ddgi_probes:        StorageBuffer            // DDGIProbe[cascade][N]
wsrc_grid:          StorageBuffer            // WSRC HashMap flattened
dirty_voxels:       StorageBuffer            // voxels modificados este frame
```

---

## Orden de passes en el render graph (por frame)

```
[CPU pre-frame]
  1. Voxel edits → update HashDAG CPU → mark dirty_voxels
  2. Mark nearby DDGI probes as dirty
  3. Frustum cull chunks (nivel 4, 40m) using existing Frustum
  4. Update Light BVH (sol + emisivos en chunks activos)
  5. Upload dirty DAG nodes to GPU

[GPU — ComputePasses via extra_passes]
  Pass 0: VoxelGpuUpload     → sync dirty DAG nodes + occupancy + invalidation_mask
  Pass 1: HddaPrimaryPass    → HDDA raymarching → GBuffer + fog
  Pass 2: ReStirCandidates   → 7 candidatos por pixel
  Pass 3: ReStirVisibility   → shadow ray del ganador
  Pass 4: ReStirReuse        → temporal + spatial reuse
  Pass 5: GiPass             → SSRC + DDGI lookup + WSRC
  Pass 6: SvgfTemporal       → acumulación temporal
  Pass 7: SvgfWavelet        → filtro à-trous × 5 (loop interno en el shader)
  Pass 8: SvgfComposite      → reconstrucción final → HDR texture
  Pass 9: TaaPass            → TAA jitter + accumulate → output final

[Existente — sin cambios]
  PostProcessPass            → ACES tonemap sobre HDR output
```

**Clave de integración:** Cada pass escribe en la HDR texture del renderer existente
(`world_pass.hdr_texture`) o en storage textures propias. El `SvgfComposite` es el
último que escribe en el HDR, y `PostProcessPass` lo tonemapea igual que siempre.

---

## Fases de implementación

### ✅ FASE 0 — Completada: Análisis (esta sesión)
- Auditoría del engine existente
- Diseño de arquitectura completa
- Creación de este roadmap

---

### ✅ FASE 1 — Completada: Crate base + HashDAG CPU
**Resultado:** 22/22 tests pasan. `cargo build -p ferrous_voxels` limpio (0 errores).

**Implementado:**
- `crates/ferrous_voxels/` crate en workspace (sin wgpu, solo CPU)
- `dag/node.rs` → `Voxel` (pack/unpack u32), `VoxelFlags` (bitflags), `DAGNode` (8 children + masks)
- `dag/bit_grid.rs` → `BitGrid3D` (flat `Vec<u64>`), `LevelGrids` (occupancy + emissivity)
- `dag/hash_dag.rs` → `HashDAG` 5 niveles, `NodePool` con deduplicación content-addressed
  (`HashMap<u64, Vec<u32>>` para manejar hash collisions), path-copy en `set_recursive`,
  dirty tracking `HashSet<(level, node_idx)>`, `take_dirty_nodes()`/`take_dirty_chunks()`
- `world/chunk_manager.rs` → `ChunkAABB`, `ChunkManager`, dirty/removed sets
- `world/voxel_edit.rs` → `VoxelWorld` (set_voxel, destroy_voxel, fill_box, destroy_sphere, etc.)

**Decisión de diseño — niveles:**
`LEVEL_SIZES = [1, 2, 4, 8, 16]` (factor 2 por nivel = octree binario estándar correcto).
Cada nivel divide en 8 octantes (2×2×2). Chunk raíz = 16 voxels por eje.
**Phase 3 extenderá a 13 niveles** (`LEVEL_SIZES[12] = 4096 cm ≈ 40m`) para el mundo completo;
la mecánica del DAG es agnóstica al número de niveles.

---

### ✅ FASE 2 — Completada: GPU upload + VoxelGpuUploadPass
**Resultado:** `cargo build -p ferrous_voxels --features ferrous_voxels/gpu` — limpio.
22/22 tests CPU siguen pasando (feature gate funciona correctamente).

**Implementado:**
- `Cargo.toml` — feature `gpu` gates `wgpu` (workspace), `bytemuck = "1.14"`, `ferrous_render_graph`
- `dag/gpu_types.rs` — `GpuDagNode` ([u32;8] children + 2 mask u32 = 40 bytes, `Pod+Zeroable`),
  `GpuChunkRoot` (cx,cy,cz: i32 + root_idx: u32 = 16 bytes), `GpuPackedVoxel = u32`
- `buffers/persistent.rs` — `PersistentBuffers`: staging (`MAP_WRITE|COPY_SRC`) + device-local
  (`STORAGE|COPY_DST`) buffers for nodes and roots; auto-grow on overflow
- `dag/gpu_upload.rs` — `DagGpuSync::prepare()`: full snapshot of all levels into flat
  byte slice; `LevelOffsets` tracks per-level base indices in the concatenated SSBO
- `passes/gpu_upload_pass.rs` — `VoxelGpuUploadPass` implements `RenderPass`:
  - `on_attach`: allocates `PersistentBuffers`
  - `prepare`: calls `DagGpuSync::prepare`, `queue.write_buffer` staging buffers
  - `execute`: records `encoder.copy_buffer_to_buffer` staging → device-local

**SSBO layout (for Phase 3 WGSL shader):**
```wgsl
@group(0) @binding(0) var<storage, read> dag_nodes : array<GpuDagNode>;
@group(0) @binding(1) var<storage, read> roots     : array<GpuChunkRoot>;
// GpuDagNode: children[8]: u32, occupancy_mask: u32, emissive_mask: u32
// GpuChunkRoot: cx: i32, cy: i32, cz: i32, root_idx: u32
```

**Integration usage:**
```rust
let world = Arc::new(Mutex::new(VoxelWorld::new()));
let pass = VoxelGpuUploadPass::new(Arc::clone(&world));
renderer.add_pass(Box::new(pass));
```

---

### ✅ FASE 3 — HDDA Primary Pass (raymarching básico)
**Objetivo:** Un compute shader que lanza un rayo por pixel y produce un G-Buffer
con worldPos, normalID, albedo, depth. Sin iluminación aún — solo visualización
de normales para validar el traversal.

**Archivos creados:**
```
assets/shaders/voxels/hdda_primary.wgsl      ← chunk-level DDA + 13-level DAG descent
crates/ferrous_voxels/src/passes/hdda_pass.rs ← HddaPrimaryPass implements RenderPass
```

**Cambios:**
- `VoxelGpuUploadPass.buffers` promovido a `Arc<Mutex<PersistentBuffers>>` para
  ser compartido con `HddaPrimaryPass` vía `shared_buffers()`.
- Bind group layout split: group 0 = SSBOs + uniforms, group 1 = storage textures.
- `GpuCameraUniform` (128 bytes, bytemuck `Pod`) para el uniform de cámara.
- `GpuLevelOffsets` (64 bytes, 13 + 3 pad u32) para offsets de nivel.
- Dispatch `ceil(W/8) × ceil(H/8)` workgroups de 8×8.

**Estado:** ✅ `cargo build -p ferrous_voxels --features ferrous_voxels/gpu` — 0 warnings, 0 errors.
22/22 tests pass.

---

### 🔲 FASE 4 — Iluminación directa simple (sol + shadow ray)
**Objetivo:** Luz directa del sol con shadow rays hardcodeados.
Sin ReSTIR aún. Solo para validar que el shadow HDDA funciona.

**Archivos a crear:**
```
assets/shaders/voxels/hdda_shadow.wgsl
assets/shaders/voxels/direct_light_simple.wgsl
crates/ferrous_voxels/src/passes/direct_light_pass.rs  (temporal, se reemplaza en Fase 6)
```

**Criterio de éxito:**
- Luz y sombras duras visibles en el mundo de voxels
- Sin flickering (1 shadow ray fijo por pixel)

---

### 🔲 FASE 5 — Buffers persistentes + motion vectors
**Objetivo:** Infraestructura temporal: G-Buffer history, confidence buffer,
motion vectors. Reprojección básica para el SVGF.

**Archivos a crear/modificar:**
```
crates/ferrous_voxels/src/buffers/persistent.rs  (completar)
assets/shaders/voxels/motion_vectors.wgsl
```

**Criterio de éxito:**
- `PersistentBuffers` asigna todas las texturas correctamente (resize-aware)
- Motion vectors visibles como debug output (rojo=X, verde=Y)
- G-Buffer history se repropaga correctamente con movimiento de cámara

---

### 🔲 FASE 6 — ReSTIR DI
**Objetivo:** Iluminación directa con resampling. 7 candidatos por pixel,
shadow ray del ganador, reuso temporal (20 frames) + espacial (6 vecinos).

**Archivos a crear:**
```
assets/shaders/voxels/restir_candidates.wgsl
assets/shaders/voxels/restir_visibility.wgsl
assets/shaders/voxels/restir_reuse.wgsl
crates/ferrous_voxels/src/lighting/mod.rs
crates/ferrous_voxels/src/lighting/light_bvh.rs
crates/ferrous_voxels/src/passes/restir_pass.rs
```

**Criterio de éxito:**
- Iluminación directa suave con penumbras correctas
- Voxels emisivos iluminan su entorno
- Sin artefactos de ghosting visibles con cámara estática

---

### 🔲 FASE 7 — SVGF Denoiser
**Objetivo:** Convertir la imagen ruidosa de ReSTIR en output limpio.
Acumulación temporal + wavelet à-trous 5 iteraciones.

**Archivos a crear:**
```
assets/shaders/voxels/svgf_temporal.wgsl
assets/shaders/voxels/svgf_wavelet.wgsl
assets/shaders/voxels/svgf_composite.wgsl
crates/ferrous_voxels/src/passes/svgf_pass.rs
```

**Criterio de éxito:**
- 1spp ReSTIR DI produce imagen limpia después del denoiser
- Sin ghosting excesivo en cámara en movimiento
- Preserva detalles en bordes (el wavelet respeta normales y profundidad)

---

### 🔲 FASE 8 — DDGI Probe Grid (GI baja frecuencia)
**Objetivo:** Grilla 3D de probes de irradiancia, 3 cascades, 200 probes/frame budget,
32 rayos/probe. Sampling con Chebyshev visibility test.

**Archivos a crear:**
```
assets/shaders/voxels/gi_ddgi_update.wgsl
assets/shaders/voxels/gi_ddgi_sample.wgsl
crates/ferrous_voxels/src/lighting/ddgi.rs
crates/ferrous_voxels/src/passes/gi_pass.rs  (parcial)
```

**Criterio de éxito:**
- Zonas en sombra reciben luz indirecta del cielo y de superficies iluminadas
- Los probes se actualizan en async compute sin stutter
- 3 cascades funcionan: interior (2m), exterior cercano (5m), horizonte (20m)

---

### 🔲 FASE 9 — SSRC (Screen Space Radiance Cache)
**Objetivo:** Primer bounce GI de alta frecuencia usando el G-Buffer actual.
2 rayos cortos por pixel con fallback a DDGI probes.

**Archivos a crear:**
```
assets/shaders/voxels/gi_ssrc.wgsl
(completar crates/ferrous_voxels/src/passes/gi_pass.rs)
```

**Criterio de éxito:**
- Reflexiones de color bleeding visibles (pared roja tiñe el suelo)
- Se combina visualmente con el output de DDGI

---

### 🔲 FASE 10 — WSRC (World Space Radiance Cache)
**Objetivo:** Cache de bounces secundarios en espacio mundial. HashMap GPU,
~2M entries, LRU eviction.

**Archivos a crear:**
```
assets/shaders/voxels/gi_wsrc.wgsl
crates/ferrous_voxels/src/cache/mod.rs
crates/ferrous_voxels/src/cache/wsrc.rs
```

**Criterio de éxito:**
- La GI de los probes usa el WSRC para bounces secundarios (sin recursión)
- Los cache misses son raros en frames estacionarios
- La VRAM usada por el WSRC no supera 128MB

---

### 🔲 FASE 11 — Volumétricos
**Objetivo:** Fog/polvo/humo usando la textura 3D de densidad.
In-scattering del sol con phase function Henyey-Greenstein.
El fog fue acumulado durante el HDDA primario → costo casi cero.

**Archivos a crear:**
```
assets/shaders/voxels/volumetrics.wgsl
crates/ferrous_voxels/src/passes/volumetrics_pass.rs
```

**Criterio de éxito:**
- Niebla volumétrica visible con gradiente correcto de densidad
- God rays del sol visibles a través de huecos en la geometría

---

### 🔲 FASE 12 — TAA + integración final
**Objetivo:** TAA con jitter Halton 2x2, acumulación con motion vectors,
sharpen suave. Integración completa en el pipeline del engine.

**Archivos a crear:**
```
assets/shaders/voxels/taa.wgsl
crates/ferrous_voxels/src/passes/taa_pass.rs
crates/ferrous_voxels/src/lib.rs  (VoxelRenderer API pública final)
```

**Modificaciones:**
```
ferrous_app/src/builder.rs  → AppMode::VoxelGame (nuevo modo)
ferrous_app/src/runner/frame.rs  → integrar VoxelRenderer en el frame loop
```

**Criterio de éxito:**
- Frame completo funciona: HDDA → ReSTIR → GI → SVGF → TAA → ACES
- Escena de demo: room con voxels, voxel emisivo, destrucción en tiempo real
- 60fps en RTX 3070 a 1080p interna con TAA

---

### 🔲 FASE 13 — Destrucción dinámica sin stutter
**Objetivo:** Voxel edits en tiempo real (destruction, construction) sin frame drops.
Streaming de chunks desde RAM. Invalidation mask correcta.

**Criterio de éxito:**
- Explosión que destruye 10,000 voxels por frame sin frame drops
- La GI se actualiza correctamente después de la destrucción (probes dirty → refresh)
- No hay artefactos visuales en el frame de la edición

---

### 🔲 FASE 14 — Optimización + DLSS/FSR (stretch goal)
**Objetivo:** Tile classification, render a resolución reducida, upscale con FSR 2/3.

**Nota:** DLSS requiere NVIDIA SDK cerrado y no es compatible directo con wgpu.
FSR 2 tiene implementaciones de compute shader que sí se pueden portar a WGSL.
Evaluar en esta fase si usar FSR 2 compute o TAA upscale manual.

---

## Presupuesto final de rayos (objetivo a 1080p)

```
Primarios (con repro)       800K    20%
Shadow (ReSTIR)             1.6M    40%
GI SSRC (cortos)            400K    10%
Probe updates (async)       6.4K    <1%
WSRC miss (raro)            ~200K    5%
─────────────────────────────────────
Total efectivo              ~3M
Path tracer naive            ~50M
─────────────────────────────────────
Ahorro objetivo              ~17x
```

---

## Convenciones de código

- Todos los crates nuevos usan `edition = "2021"`, `wgpu = "23.0"`, `glam = "0.29"`
- Los shaders WGSL viven en `assets/shaders/voxels/` y se incluyen con `include_str!`
- Cada `*Pass` struct implementa `ferrous_render_graph::RenderPass`
- Los buffers persistentes se recrían en `on_resize` del pass correspondiente
- Los parámetros por-frame se suben en `prepare`, los draw/dispatch en `execute`
- Usar `Arc<wgpu::Buffer>` para buffers compartidos entre passes (igual que el engine)

---

## Cómo leer este roadmap en cada sesión nueva

1. Leer la sección **Estado actual del engine** para recordar qué existe
2. Buscar la primera fase con 🔲 (no completada)
3. Leer **todos** sus archivos a crear + criterio de éxito antes de escribir código
4. Al completar una fase, **cambiar 🔲 por ✅** y añadir notas si hubo cambios de diseño
5. Si se cambió algo significativo del diseño durante la implementación, actualizar
   la sección de arquitectura correspondiente en este archivo

---

*Roadmap creado: 2026-03-12 | Engine base: wgpu 23.0, Rust edition 2021*
