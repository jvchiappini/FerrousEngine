// Synthetic per-pixel workload for `examples/partial_redraw.rs`.
//
// The fragment shader runs a fixed number of iterations of cheap arithmetic and
// returns a premultiplied colour with a small alpha, so the pipeline writes
// every pixel it covers (the blend never drops the fragment).  The point is not
// what it draws but how much per-pixel work it does: a representative heavy UI
// overlay, not a real widget.

struct Varying {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Varying {
    // Full-screen triangle in clip space.
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    var out: Varying;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(input: Varying) -> @location(0) vec4<f32> {
    var acc = vec3<f32>(0.0);
    var px = input.position.xy;
    for (var i = 0u; i < 64u; i = i + 1u) {
        let p = px * 0.031 + f32(i) * 0.17;
        acc = acc + vec3<f32>(
            sin(p.x) * cos(p.y),
            sin(p.x + p.y) * 0.5,
            cos(p.x - p.y) * 0.25,
        );
    }
    acc = abs(acc) * 0.05;
    // Premultiplied: rgb is already scaled by alpha, matching
    // `BlendState::PREMULTIPLIED_ALPHA_BLENDING`.
    return vec4<f32>(acc * 0.2, 0.2);
}
