# ROADMAP FERROUS ENGINE DX 1000X

## 1. Ferrous Fiber (React Reconciler Declarativo)
Convertir la API imperativa en un ecosistema de componentes React declarativo, administrando el ciclo de vida y montaje como `react-three-fiber`.

## 2. Tipado Estricto y Generación Automática (Zero-Manual-Types)
Parseo automático del AST desde Rust para exportar los comandos de `JsCommand` directamente a `.d.ts`. Autocompletado perfecto.

## 3. Inspector Visual y Hot-Module-Replacement (HMR) para Shaders
Hot-Reload de `.wgsl` sin recargar la página. Un panel inspector sobre el DOM en modo dev para editar luces, materiales e instanciar código final ("Export to Code").

## 4. Hook de Assets Tipados (GLTF to Code)
CLI que lea los GLTF del proyecto y genere código React tipado (ej: `<Mesh geometry={nodes.Pipes} material={materials.MetalRust} />`).

## 5. Suspense y Promesas Nativas Visuales
Integrar `engine.loadTexture()` y `engine.loadModel()` con `<Suspense>` de React para manejar estados de carga nativos ("Loading...").

