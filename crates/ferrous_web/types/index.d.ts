/**
 * Ferrous Web Engine - TypeScript Definitions
 * 
 * This file provides high-level type definitions for the Ferrous engine,
 * covering the WASM bridge, scene persistence formats, and component structures.
 * 
 * These definitions match the camelCase exported API.
 */

export namespace Ferrous {
    /**
     * The main entry point for the engine in the browser.
     */
    export interface Engine {
        /**
         * Initialize the engine and mount it to a canvas.
         */
        mountAndRun(): void;

        /**
         * Release all engine resources.
         */
        dispose(): void;

        // ── Primitives ────────────────────────────────────────────────────────

        /** Create a new entity with a primitive box shape. */
        createBox(name: string, x: number, y: number, z: number, sx: number, sy: number, sz: number, r: number, g: number, b: number): Entity;

        /** Create a new entity with a primitive sphere shape. */
        createSphere(name: string, x: number, y: number, z: number, radius: number, segments: number, r: number, g: number, b: number): Entity;

        /** Create a cylinder, cone, or frustum. */
        createCylinder(name: string, x: number, y: number, z: number, radiusTop: number, radiusBottom: number, height: number, radialSegments: number, r: number, g: number, b: number): Entity;

        /** Create a cone. */
        createCone(name: string, x: number, y: number, z: number, radius: number, height: number, radialSegments: number, r: number, g: number, b: number): Entity;

        /** Create a torus (donut). */
        createTorus(name: string, x: number, y: number, z: number, radius: number, tube: number, radialSegments: number, tubularSegments: number, r: number, g: number, b: number): Entity;

        /** Create a capsule. */
        createCapsule(name: string, x: number, y: number, z: number, radius: number, height: number, radialSegments: number, capSegments: number, r: number, g: number, b: number): Entity;

        /** Create a flat plane in the XZ plane. */
        createPlane(name: string, x: number, y: number, z: number, width: number, height: number, widthSegments: number, heightSegments: number, r: number, g: number, b: number): Entity;

        /** Create a flat circle disc. */
        createCircle(name: string, x: number, y: number, z: number, radius: number, segments: number, r: number, g: number, b: number): Entity;

        /** Create a ring (annulus). */
        createRing(name: string, x: number, y: number, z: number, innerRadius: number, outerRadius: number, segments: number, rings: number, r: number, g: number, b: number): Entity;

        /** Spawn a custom entity type or asset mesh. */
        spawnEntity(name: string, kind: string, x: number, y: number, z: number, r: number, g: number, b: number): Entity;

        // ── Scene ─────────────────────────────────────────────────────────────

        /** Create a new empty scene and return its ID. */
        createScene(): number;

        /** Switch the active scene. */
        setActiveScene(sceneId: number): void;

        /** Clear all entities from the current world. */
        clearWorld(): void;

        /** Find and remove an entity by name. */
        removeEntity(name: string): void;

        /** Toggle entity visibility. */
        setVisible(name: string, visible: boolean): void;

        /** Export the scene to JSON. */
        exportScene(): Promise<string>;

        /** Import a scene from JSON. */
        importScene(json: string): void;

        // ── Camera ────────────────────────────────────────────────────────────

        /** Set camera position and target. */
        setCamera(ex: number, ey: number, ez: number, tx: number, ty: number, tz: number): void;

        /** Set camera control mode ('fly' | 'orbit' | 'none'). */
        setCameraControlMode(mode: 'fly' | 'orbit' | 'none'): void;

        /** Set camera movement and look parameters. */
        setCameraParams(speed: number, sensitivity: number): void;

        /** Set vertical FOV in degrees. */
        setCameraFov(fovDegrees: number): void;

        // ── Lighting ──────────────────────────────────────────────────────────

        /** Add a point light to the scene. */
        addPointLight(name: string, x: number, y: number, z: number, r: number, g: number, b: number, intensity: number, range: number): void;

        /** Set the single global directional light (sun). */
        setDirectionalLight(dx: number, dy: number, dz: number, r: number, g: number, b: number, intensity: number): void;

        /** Set global ambient light level. */
        setAmbientLight(r: number, g: number, b: number, intensity: number): void;

        // ── Environment ───────────────────────────────────────────────────────

        /** Set distance fog parameters. */
        setEnvironment(r: number, g: number, b: number, density: number): void;

        /** Set tone mapping exposure. */
        setExposure(exposure: number): void;

        /** Set viewport background color (clear color). */
        setBackground(r: number, g: number, b: number): void;

        // ── Assets ────────────────────────────────────────────────────────────

        /** Pre-load a texture and return a promise for its internal ID. */
        loadTexture(url: string): Promise<number>;

        /** Pre-load a GLTF model. */
        loadModel(url: string): Promise<number>;

        // ── Plugins & Debug ───────────────────────────────────────────────────

        /** Register custom JS plugin hooks. */
        registerPlugin(name: string, onUpdate?: (dt: number) => void, onSyncWorld?: (world: any) => void): void;

        /** Enable/Disable debug HUD. */
        setDebugMode(enabled: boolean): void;

        /** Get current performance metrics. */
        getMetricsJson(): string;

        /** Enable/disable specific built-in plugins (e.g. 'terrain', 'sky'). */
        enablePlugin(name: string): void;
        disablePlugin(name: string): void;
    }

    /**
     * A handle to an entity in the 3D world. Chainable methods.
     */
    export interface Entity {
        setPosition(x: number, y: number, z: number): Entity;
        setRotation(rx: number, ry: number, rz: number): Entity;
        setScale(sx: number, sy: number, sz: number): Entity;
        setVisible(visible: boolean): Entity;
        setColor(r: number, g: number, b: number): Entity;
        setMaterial(r: number, g: number, b: number, metal: number, rough: number): Entity;
        remove(): void;
    }

    // ── Persistence Models ────────────────────────────────────────────────────

    export interface SceneBlueprint {
        name: string;
        entities: SceneElement[];
        directional_light?: DirectionalLight;
    }

    export interface SceneElement {
        id: number;
        name: string;
        transform: Transform;
        material: Material;
        kind: ElementKind;
        tags: string[];
        visible: boolean;
        point_light?: PointLight;
    }

    export interface Transform {
        position: [number, number, number];
        rotation: [number, number, number, number];
        scale: [number, number, number];
    }

    export interface Material {
        handle: number;
        descriptor: MaterialDescriptor;
    }

    export interface MaterialDescriptor {
        base_color: [number, number, number, number];
        metallic: number;
        roughness: number;
        clearcoat: number;
        clearcoat_roughness: number;
        opacity: number;
        albedo_tex?: number;
    }

    export type ElementKind =
        | { Cube: { half_extents: [number, number, number] } }
        | { Sphere: { radius: number, latitudes: number, longitudes: number } }
        | { Cylinder: { radius_top: number, radius_bottom: number, height: number, radial_segments: number, height_segments: number, open_ended: boolean } }
        | { Torus: { radius: number, tube: number, radial_segments: number, tubular_segments: number } }
        | { Plane: { width: number, height: number, width_segments: number, height_segments: number } }
        | { Capsule: { radius: number, height: number, radial_segments: number, cap_segments: number } }
        | { Circle: { radius: number, segments: number } }
        | { Ring: { inner_radius: number, outer_radius: number, segments: number, rings: number } }
        | { Mesh: { asset_key: string } }
        | { PointLight: { radius: number, intensity: number } }
        | 'Empty';

    export interface DirectionalLight {
        direction: [number, number, number];
        color: [number, number, number];
        intensity: number;
    }

    export interface PointLight {
        color: [number, number, number];
        intensity: number;
        radius: number;
    }
}
