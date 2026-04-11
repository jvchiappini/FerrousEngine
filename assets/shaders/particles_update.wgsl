// GPU Particle Update Shader (Compute)

struct Particle {
    position : vec3<f32>,
    life     : f32,
    velocity : vec3<f32>,
    size     : f32,
};

struct EmitterParams {
    origin            : vec3<f32>,
    spawn_count       : u32,
    direction         : vec3<f32>,
    randomness        : f32,
    gravity           : vec3<f32>,
    lifetime          : f32,
    delta_time        : f32,
    max_particles     : u32,
    time              : f32, // For noise/random
};

@group(0) @binding(0)
var<uniform> emitter: EmitterParams;

@group(1) @binding(0)
var<storage, read_write> particles: array<Particle>;

// Atomic counter for particle allocation (optional, using simple ring buffer for now)
// In a more advanced system we'd use a dead list / alive list.

fn pcg_hash(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand_vec3(seed: u32) -> vec3<f32> {
    let h1 = pcg_hash(seed);
    let h2 = pcg_hash(h1);
    let h3 = pcg_hash(h2);
    return vec3<f32>(
        f32(h1) / 4294967295.0,
        f32(h2) / 4294967295.0,
        f32(h3) / 4294967295.0
    ) * 2.0 - 1.0;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let id = global_id.x;
    if (id >= emitter.max_particles) {
        return;
    }

    var p = particles[id];

    if (p.life > 0.0) {
        // Update existing particle
        p.velocity += emitter.gravity * emitter.delta_time;
        p.position += p.velocity * emitter.delta_time;
        p.life -= emitter.delta_time;
    } else {
        // Recycle dead particle if within spawn count this frame
        // This is a naive spawn logic for demonstration.
        // A better one would use an atomic counter.
        let spawn_threshold = emitter.spawn_count;
        // We use a pseudo-random check to see if we spawn this frame
        // (This is not perfect but works for massive systems)
        let seed = id + u32(emitter.time * 1000.0);
        if (pcg_hash(seed) % (emitter.max_particles / max(1u, emitter.spawn_count)) == 0u) {
            p.position = emitter.origin;
            let r = rand_vec3(seed);
            p.velocity = emitter.direction + r * emitter.randomness;
            p.life = emitter.lifetime * (0.8 + 0.4 * (f32(pcg_hash(seed + 1u)) / 4294967295.0));
            p.size = 1.0; 
        }
    }

    particles[id] = p;
}
