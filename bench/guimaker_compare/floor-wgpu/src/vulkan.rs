mod common;

fn main() {
    common::run(wgpu::Backends::VULKAN, "floor-wgpu-vulkan", "vulkan");
}
