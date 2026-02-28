// gestión de la textura de salida

pub struct RenderTarget {
    /// Textura que contiene el color final. Se elige
    /// `Bgra8UnormSrgb` porque la mayoría de superficies (swap
    /// chains) esperan ese formato y además lleva la corrección de
    /// espacio de color integrada.
    pub color_texture: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    /// multisampled colour texture (if sample_count > 1)
    pub msaa_texture: Option<wgpu::Texture>,
    pub msaa_view: Option<wgpu::TextureView>,

    /// Textura para el buffer de profundidad. `Depth32Float` es un
    /// formato amplio y suficientemente preciso para la mayoría de
    /// escenas 3D, además está bien soportado por WGPU/HLSL/GLSL.
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,

    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    pub sample_count: u32,
}

impl RenderTarget {
    /// Crea un `RenderTarget` con las dimensiones y formato especificados.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // the "presentable" colour texture is always single-sample;
        // MSAA is handled by a separate texture when requested.
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

        // optional multisampled target
        let (msaa_texture, msaa_view) = if sample_count > 1 {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("RenderTarget MSAA Color"),
                size,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(tex), Some(view))
        } else {
            (None, None)
        };

        // depth buffer must match whatever sampling we're going to
        // render with; if we draw into an MSAA colour target then the
        // depth buffer also has to be multisampled, otherwise the GPU
        // validation layer will complain about mismatched attachment
        // sample counts.  `resize` already handles this correctly, so
        // the only mistake was to hard‑code "1" here previously.
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RenderTarget Depth"),
            size,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            color_texture,
            color_view,
            msaa_texture,
            msaa_view,
            depth_texture,
            depth_view,
            width,
            height,
            format,
            sample_count,
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

        // colour texture is the *presentable* target and must always be
        // single‑sampled.  MSAA is performed into `msaa_texture` and
        // later resolved into this texture.  the previous version of
        // this code erroneously used `self.sample_count` here which
        // could make the image multisampled after a resize, leading to
        // validation errors when we used it as a resolve target.
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

        // recreate MSAA buffer if needed
        if self.sample_count > 1 {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("RenderTarget MSAA Color"),
                size,
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.msaa_view = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
            self.msaa_texture = Some(tex);
        }

        self.depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RenderTarget Depth"),
            size,
            mip_level_count: 1,
            sample_count: self.sample_count,
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
