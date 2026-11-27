mod common;

fn main() {
    common::run(wgpu::Backends::DX12, "floor-wgpu-dx12", "dx12");
}
