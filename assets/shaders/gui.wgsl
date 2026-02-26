// Shader para renderizar quads de UI. Las coordenadas se pasan en píxeles
// y se convierten a clip space mediante la resolución de la pantalla.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @location(0) uv: vec2<f32>,
    @location(1) i_pos: vec2<f32>,
    @location(2) i_size: vec2<f32>,
    @location(3) i_color: vec4<f32>,
) -> VsOut {
    // uv va de (0,0) en la esquina inferior izquierda a (1,1) en la
    // esquina superior derecha del quad unitario.
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    // invertir Y para coincidir con coordenadas de pantalla típicas
    ndc.y = -ndc.y;
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.color = i_color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
