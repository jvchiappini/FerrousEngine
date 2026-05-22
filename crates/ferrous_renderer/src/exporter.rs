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

/// [ENG-API-04] Direct-to-video exporter using FFmpeg.
/// Pipes raw RGBA buffers into a spawned ffmpeg process for high-performance
/// headless video generation.
pub struct FFmpegExporter {
    process: std::process::Child,
}

impl FFmpegExporter {
    /// Spawns a new ffmpeg process configured to receive raw RGBA8 frames.
    pub fn new<P: AsRef<std::path::Path>>(
        output_path: P,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<Self, String> {
        use std::process::{Command, Stdio};

        let child = Command::new("ffmpeg")
            .args([
                "-y",                   // Overwrite existing file
                "-f", "rawvideo",       // Input format
                "-pixel_format", "rgba",
                "-video_size", &format!("{}x{}", width, height),
                "-framerate", &fps.to_string(),
                "-i", "-",              // Read from stdin
                "-c:v", "libx264",      // Use H.264 codec
                "-pix_fmt", "yuv420p",  // Compatible pixel format for most players
                "-preset", "veryfast",  // Balance speed/compression
                output_path.as_ref().to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start FFmpeg: {}. Ensure ffmpeg is in PATH.", e))?;

        Ok(Self { process: child })
    }

    /// Finishes the process and waits for encoding to complete.
    pub fn finish(mut self) -> Result<(), String> {
        // Drop stdin to signal EOF to ffmpeg
        drop(self.process.stdin.take());
        let status = self.process.wait().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("FFmpeg failed with exit code: {:?}", status.code()))
        }
    }
}

impl FrameExporter for FFmpegExporter {
    fn push_frame(&mut self, rgba_data: &[u8], _width: u32, _height: u32) -> Result<(), String> {
        use std::io::Write;
        let stdin = self.process.stdin.as_mut().ok_or("FFmpeg stdin not available")?;
        stdin.write_all(rgba_data).map_err(|e| format!("Pipe error: {}", e))?;
        Ok(())
    }
}
