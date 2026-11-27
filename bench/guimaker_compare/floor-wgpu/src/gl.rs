mod common;

fn main() {
    common::run(wgpu::Backends::GL, "floor-wgpu-gl", "gl");
}
