use std::fs;
use std::path::{Path, PathBuf};

/// [ENG-API-03] Contrato general de exportación.
/// Una interfaz genérica para exportar/codificar fotogramas RAW obtenidos desde la RAM
/// (por ejemplo, hacia un archivo MKV o una carpeta de imágenes).
pub trait FrameExporter {
    /// Inserta un fotograma RGBA puro del buffer headless en el flujo de salida.
    fn push_frame(&mut self, rgba_data: &[u8], width: u32, height: u32) -> Result<(), String>;
}

/// Implementación básica y funcional de `FrameExporter` usando la librería nativa `image`.
/// Guarda los fotogramas secuencialmente en carpetas (ej. output/frame_0000.png).
#[cfg(feature = "image")]
pub struct ImageFolderExporter {
    folder: PathBuf,
    frame_count: usize,
}

#[cfg(feature = "image")]
impl ImageFolderExporter {
    /// Crea un nuevo exportador hacia una carpeta dada, generándola si no existe.
    pub fn new<P: AsRef<Path>>(folder: P) -> Self {
        let path = folder.as_ref().to_path_buf();
        if !path.exists() {
            let _ = fs::create_dir_all(&path);
        }
        Self {
            folder: path,
            frame_count: 0,
        }
    }
}

#[cfg(feature = "image")]
impl FrameExporter for ImageFolderExporter {
    fn push_frame(&mut self, rgba_data: &[u8], width: u32, height: u32) -> Result<(), String> {
        let file_path = self.folder.join(format!("frame_{:04}.png", self.frame_count));
        
        match image::save_buffer(
            file_path,
            rgba_data,
            width,
            height,
            image::ColorType::Rgba8,
        ) {
            Ok(_) => {
                self.frame_count += 1;
                Ok(())
            }
            Err(e) => Err(format!("Failed to save image: {}", e)),
        }
    }
}
