// Shader muy básico que genera un triángulo coloreado sin necesidad de
// buffers de vértices. Los colores y posiciones están "hardcoded" en el
// propio shader para mantener el ejemplo lo más simple posible.

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) color : vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) id : u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, -0.5),
    );
    var colors = array<vec3<f32>, 3>(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0),
    );
    var out : VsOut;
    out.clip_pos = vec4<f32>(positions[id], 0.0, 1.0);
    out.color = colors[id];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
