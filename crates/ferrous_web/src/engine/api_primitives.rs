use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::entity::JsEntity;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = createSprite2d)]
    pub fn create_sprite2d(
        &self,
        name: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        z_index: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
        texture_id: Option<u32>,
    ) -> JsEntity {
        self.push_command(JsCommand::CreateSprite2d {
            name: name.clone(),
            position: [x, y],
            size: [w, h],
            z_index,
            color: [r, g, b, a],
            texture_id,
        });
        JsEntity::new(name, self.tx.clone())
    }

    #[wasm_bindgen(js_name = setCamera2d)]
    pub fn set_camera2d(&self, zoom: f32, clear_r: f32, clear_g: f32, clear_b: f32, clear_a: f32) {
        self.push_command(JsCommand::SetCamera2d {
            zoom,
            clear_color: Some([clear_r, clear_g, clear_b, clear_a]),
        });
    }

    #[wasm_bindgen(js_name = createBox)]
    pub fn create_box(
        &self, name: String,
        position: Vec<f32>,
        size: Vec<f32>,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let s = [size.get(0).copied().unwrap_or(1.0), size.get(1).copied().unwrap_or(1.0), size.get(2).copied().unwrap_or(1.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];
        
        self.push_command(JsCommand::CreateBox {
            name: name.clone(),
            position: p,
            size: [s[0].max(0.01), s[1].max(0.01), s[2].max(0.01)],
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createSphere)]
    pub fn create_sphere(
        &self, name: String,
        position: Vec<f32>,
        radius: f32, segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreateSphere {
            name: name.clone(),
            position: p,
            radius: radius.max(0.01),
            segments: segments.max(3),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCylinder)]
    pub fn create_cylinder(
        &self, name: String,
        position: Vec<f32>,
        radius_top: f32, radius_bottom: f32,
        height: f32, radial_segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreateCylinder {
            name: name.clone(),
            position: p,
            radius_top: radius_top.max(0.0),
            radius_bottom: radius_bottom.max(0.001),
            height: height.max(0.01),
            radial_segments: radial_segments.max(3),
            open_ended: false,
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCone)]
    pub fn create_cone(
        &self, name: String,
        position: Vec<f32>,
        radius: f32, height: f32, radial_segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreateCone {
            name: name.clone(),
            position: p,
            radius: radius.max(0.001),
            height: height.max(0.01),
            radial_segments: radial_segments.max(3),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createTorus)]
    pub fn create_torus(
        &self, name: String,
        position: Vec<f32>,
        radius: f32, tube: f32,
        radial_segments: u32, tubular_segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreateTorus {
            name: name.clone(),
            position: p,
            radius: radius.max(0.01),
            tube: tube.max(0.001).min(radius - 0.001),
            radial_segments: radial_segments.max(3),
            tubular_segments: tubular_segments.max(3),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCapsule)]
    pub fn create_capsule(
        &self, name: String,
        position: Vec<f32>,
        radius: f32, height: f32,
        radial_segments: u32, cap_segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreateCapsule {
            name: name.clone(),
            position: p,
            radius: radius.max(0.001),
            height: height.max(0.0),
            radial_segments: radial_segments.max(3),
            cap_segments: cap_segments.max(2),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createPlane)]
    pub fn create_plane(
        &self, name: String,
        position: Vec<f32>,
        width: f32, height: f32,
        width_segments: u32, height_segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreatePlane {
            name: name.clone(),
            position: p,
            width: width.max(0.001),
            height: height.max(0.001),
            width_segments: width_segments.max(1),
            height_segments: height_segments.max(1),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createCircle)]
    pub fn create_circle(
        &self, name: String,
        position: Vec<f32>,
        radius: f32, segments: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];

        self.push_command(JsCommand::CreateCircle {
            name: name.clone(),
            position: p,
            radius: radius.max(0.001),
            segments: segments.max(3),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createRing)]
    pub fn create_ring(
        &self, name: String,
        position: Vec<f32>,
        inner_radius: f32, outer_radius: f32,
        segments: u32, rings: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];
        let inner = inner_radius.max(0.0);
        let outer = outer_radius.max(inner + 0.001);
        self.push_command(JsCommand::CreateRing {
            name: name.clone(),
            position: p,
            inner_radius: inner,
            outer_radius: outer,
            segments: segments.max(3),
            rings: rings.max(1),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = createText3D)]
    pub fn create_text3d(
        &self, name: String,
        text: String,
        font_data: &[u8],
        position: Vec<f32>,
        depth: f32,
        bevel_enabled: bool,
        bevel_thickness: f32,
        bevel_size: f32,
        quality: u32,
        color: Vec<f32>,
    ) -> JsEntity {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];
 
        self.push_command(JsCommand::CreateText3D {
            name: name.clone(),
            text,
            font_data: font_data.to_vec(),
            position: p,
            depth: depth.max(0.001),
            bevel_enabled,
            bevel_thickness,
            bevel_size,
            quality: quality.max(1),
            color: c,
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }
}
