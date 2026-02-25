// gestión de la textura de salida

pub struct RenderTarget {
    /// Textura que contiene el color final. Se elige
    /// `Bgra8UnormSrgb` porque la mayoría de superficies (swap
    /// chains) esperan ese formato y además lleva la corrección de
    /// espacio de color integrada.
    pub color_texture: wgpu::Texture,
    pub color_view: wgpu::TextureView,

    /// Textura para el buffer de profundidad. `Depth32Float` es un
    /// formato amplio y suficientemente preciso para la mayoría de
    /// escenas 3D, además está bien soportado por WGPU/HLSL/GLSL.
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,

    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

impl RenderTarget {
    /// Crea un `RenderTarget` con las dimensiones y formato especificados.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RenderTarget Color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RenderTarget Depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            color_texture,
            color_view,
            depth_texture,
            depth_view,
            width,
            height,
            format,
        }
    }

    /// Reconstruye las texturas cuando cambian las dimensiones.
    ///
    /// El `device` es necesario porque la creación de texturas ocurre
    /// en su contexto.
    pub fn resize(&mut self, device: &wgpu::Device, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }

        self.width = new_width;
        self.height = new_height;

        let size = wgpu::Extent3d {
            width: new_width,
            height: new_height,
            depth_or_array_layers: 1,
        };

        self.color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RenderTarget Color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.color_view = self
            .color_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RenderTarget Depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
    }
}
