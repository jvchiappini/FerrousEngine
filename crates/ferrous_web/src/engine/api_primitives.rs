use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::entity::JsEntity;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = createBox)]
    pub fn create_box(
        &self, name: String,
        x: f32, y: f32, z: f32,
        sx: f32, sy: f32, sz: f32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateBox {
            name: name.clone(),
            position: [x, y, z],
            size: [sx.max(0.01), sy.max(0.01), sz.max(0.01)],
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createSphere)]
    pub fn create_sphere(
        &self, name: String,
        x: f32, y: f32, z: f32,
        radius: f32, segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateSphere {
            name: name.clone(),
            position: [x, y, z],
            radius: radius.max(0.01),
            segments: segments.max(3),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCylinder)]
    pub fn create_cylinder(
        &self, name: String,
        x: f32, y: f32, z: f32,
        radius_top: f32, radius_bottom: f32,
        height: f32, radial_segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateCylinder {
            name: name.clone(),
            position: [x, y, z],
            radius_top: radius_top.max(0.0),
            radius_bottom: radius_bottom.max(0.001),
            height: height.max(0.01),
            radial_segments: radial_segments.max(3),
            open_ended: false,
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCone)]
    pub fn create_cone(
        &self, name: String,
        x: f32, y: f32, z: f32,
        radius: f32, height: f32, radial_segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateCone {
            name: name.clone(),
            position: [x, y, z],
            radius: radius.max(0.001),
            height: height.max(0.01),
            radial_segments: radial_segments.max(3),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createTorus)]
    pub fn create_torus(
        &self, name: String,
        x: f32, y: f32, z: f32,
        radius: f32, tube: f32,
        radial_segments: u32, tubular_segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateTorus {
            name: name.clone(),
            position: [x, y, z],
            radius: radius.max(0.01),
            tube: tube.max(0.001).min(radius - 0.001),
            radial_segments: radial_segments.max(3),
            tubular_segments: tubular_segments.max(3),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCapsule)]
    pub fn create_capsule(
        &self, name: String,
        x: f32, y: f32, z: f32,
        radius: f32, height: f32,
        radial_segments: u32, cap_segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateCapsule {
            name: name.clone(),
            position: [x, y, z],
            radius: radius.max(0.001),
            height: height.max(0.0),
            radial_segments: radial_segments.max(3),
            cap_segments: cap_segments.max(2),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createPlane)]
    pub fn create_plane(
        &self, name: String,
        x: f32, y: f32, z: f32,
        width: f32, height: f32,
        width_segments: u32, height_segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreatePlane {
            name: name.clone(),
            position: [x, y, z],
            width: width.max(0.001),
            height: height.max(0.001),
            width_segments: width_segments.max(1),
            height_segments: height_segments.max(1),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCircle)]
    pub fn create_circle(
        &self, name: String,
        x: f32, y: f32, z: f32,
        radius: f32, segments: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateCircle {
            name: name.clone(),
            position: [x, y, z],
            radius: radius.max(0.001),
            segments: segments.max(3),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createRing)]
    pub fn create_ring(
        &self, name: String,
        x: f32, y: f32, z: f32,
        inner_radius: f32, outer_radius: f32,
        segments: u32, rings: u32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        let inner = inner_radius.max(0.0);
        let outer = outer_radius.max(inner + 0.001);
        self.push_command(JsCommand::CreateRing {
            name: name.clone(),
            position: [x, y, z],
            inner_radius: inner,
            outer_radius: outer,
            segments: segments.max(3),
            rings: rings.max(1),
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }
}
