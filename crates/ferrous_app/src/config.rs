//! `ferrous.toml` — project-level configuration for FerrousEngine.
//!
//! Drop a `ferrous.toml` next to your executable (or at the workspace root
//! during development) and call [`AppBuilder::with_config_file`] to apply it
//! before the event loop starts.
//!
//! All sections and keys are optional — missing fields fall back to the
//! defaults already baked into [`AppConfig`].
//!
//! # Example `ferrous.toml`
//!
//! ```toml
//! [engine]
//! target_fps = 60
//! vsync      = true
//!
//! [window]
//! title  = "My Game"
//! width  = 1920
//! height = 1080
//!
//! [renderer]
//! quality = "high"   # ultra | high | medium | low | minimal
//! style   = "pbr"    # pbr | cel | flat
//! msaa    = 4
//! hdri    = "assets/skybox/outdoor.exr"
//!
//! [renderer.cel]
//! toon_levels   = 4
//! outline_width = 0.02
//!
//! [assets]
//! hot_reload = true
//! ```

use ferrous_core::RenderQuality;
use ferrous_renderer::RenderStyle;

use crate::builder::AppConfig;

// ── Deserializable sub-structs ─────────────────────────────────────────────

/// Top-level engine loop settings (inside `[engine]`).
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct EngineSection {
    /// Target frames per second.  `0` means unlimited.
    pub target_fps: Option<u32>,
    /// Enable vertical sync.
    pub vsync: Option<bool>,
}

/// Window settings (inside `[window]`).
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct WindowSection {
    pub title: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Whether the window can be resized by the user.
    pub resizable: Option<bool>,
    /// Whether to show OS-native decorations (title bar, minimize / maximise /
    /// close buttons).  Set to `false` for a fully-custom borderless window.
    pub decorations: Option<bool>,
}

/// Cel-shading sub-config (inside `[renderer.cel]`).
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct CelSection {
    /// Number of discrete toon shading bands (2–8).
    pub toon_levels: Option<u32>,
    /// Inverted-hull outline thickness in world space.
    pub outline_width: Option<f32>,
}

/// Renderer settings (inside `[renderer]`).
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct RendererSection {
    /// Quality preset string: `"ultra"`, `"high"`, `"medium"`, `"low"`, `"minimal"`.
    pub quality: Option<String>,
    /// Render style string: `"pbr"`, `"cel"`, `"flat"`.
    pub style: Option<String>,
    /// MSAA sample count (`1`, `2`, or `4`).
    pub msaa: Option<u32>,
    /// Path to an HDR environment map (`.exr` or `.hdr`) for IBL.
    pub hdri: Option<String>,
    /// Cel-shading sub-section.
    pub cel: Option<CelSection>,
}

/// Asset-server settings (inside `[assets]`).
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetsSection {
    /// Enable file-watcher hot-reload on desktop.
    pub hot_reload: Option<bool>,
}

// ── Top-level config ───────────────────────────────────────────────────────

/// Parsed representation of a `ferrous.toml` file.
///
/// Every field is optional — call [`EngineConfig::apply_to`] to overlay
/// the parsed values onto an existing [`AppConfig`], leaving unspecified
/// fields at their defaults.
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(default)]
pub struct EngineConfig {
    pub engine:   EngineSection,
    pub window:   WindowSection,
    pub renderer: RendererSection,
    pub assets:   AssetsSection,
}

impl EngineConfig {
    /// Overlay the values parsed from `ferrous.toml` onto `cfg`.
    ///
    /// Only fields that are explicitly set in the TOML are applied;
    /// everything else retains its current value.
    pub fn apply_to(&self, cfg: &mut AppConfig) {
        // [engine]
        if let Some(fps) = self.engine.target_fps {
            cfg.target_fps = if fps == 0 { None } else { Some(fps) };
        }
        if let Some(vsync) = self.engine.vsync {
            cfg.vsync = vsync;
        }

        // [window]
        if let Some(ref title) = self.window.title {
            cfg.title = title.clone();
        }
        if let Some(w) = self.window.width {
            cfg.width = w;
        }
        if let Some(h) = self.window.height {
            cfg.height = h;
        }
        if let Some(r) = self.window.resizable {
            cfg.resizable = r;
        }
        if let Some(d) = self.window.decorations {
            cfg.decorations = d;
        }

        // [renderer] quality
        if let Some(ref q) = self.renderer.quality {
            if let Some(quality) = RenderQuality::from_str(q) {
                cfg.render_quality = quality;
                // Let quality drive MSAA if not explicitly set.
                if self.renderer.msaa.is_none() {
                    cfg.sample_count = quality.msaa_sample_count();
                }
            }
        }

        // [renderer] msaa (explicit override wins over quality-derived value)
        if let Some(msaa) = self.renderer.msaa {
            cfg.sample_count = msaa.clamp(1, 4);
        }

        // [renderer] hdri
        if let Some(ref hdri) = self.renderer.hdri {
            cfg.hdri_path = Some(hdri.clone());
        }

        // [renderer] style
        if let Some(ref style_str) = self.renderer.style {
            let style = match style_str.to_ascii_lowercase().as_str() {
                "cel" | "cel_shaded" | "celshaded" => {
                    let cel = self.renderer.cel.clone().unwrap_or_default();
                    RenderStyle::CelShaded {
                        toon_levels:   cel.toon_levels.unwrap_or(4),
                        outline_width: cel.outline_width.unwrap_or(0.0),
                    }
                }
                "flat" | "flat_shaded" | "flatshaded" | "lowpoly" | "low_poly" => {
                    RenderStyle::FlatShaded
                }
                _ => RenderStyle::Pbr, // "pbr" or unrecognised → PBR
            };
            cfg.render_style = style;
        } else if let Some(ref cel) = self.renderer.cel {
            // [renderer.cel] present without explicit style = "cel" → infer cel
            cfg.render_style = RenderStyle::CelShaded {
                toon_levels:   cel.toon_levels.unwrap_or(4),
                outline_width: cel.outline_width.unwrap_or(0.0),
            };
        }
    }
}

// ── Loader ────────────────────────────────────────────────────────────────

/// Load and parse `ferrous.toml` from `path`.
///
/// Returns `Ok(EngineConfig)` on success, or an error if the file cannot be
/// read or contains invalid TOML.  If the file does not exist this returns
/// `Ok(EngineConfig::default())` so that projects without a config file work
/// without any extra setup.
///
/// # Example
/// ```rust,ignore
/// use ferrous_app::config::load_config;
///
/// let cfg = load_config("ferrous.toml").unwrap_or_default();
/// ```
pub fn load_config(path: &str) -> Result<EngineConfig, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(EngineConfig::default());
        }
        Err(e) => return Err(ConfigError::Io(e.to_string())),
    };

    toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))
}

// ── Error type ────────────────────────────────────────────────────────────

/// Errors that can occur while loading or parsing `ferrous.toml`.
#[derive(Debug)]
pub enum ConfigError {
    /// The file existed but could not be read (permissions, etc.).
    Io(String),
    /// The file was read but contained invalid TOML or unexpected types.
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(msg) => write!(f, "ferrous.toml I/O error: {msg}"),
            ConfigError::Parse(msg) => write!(f, "ferrous.toml parse error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_app_config() -> AppConfig {
        AppConfig::default()
    }

    // ── EngineConfig deserialization ──────────────────────────────────────

    #[test]
    fn empty_toml_gives_default_engine_config() {
        let cfg: EngineConfig = toml::from_str("").unwrap();
        assert!(cfg.engine.target_fps.is_none());
        assert!(cfg.window.title.is_none());
    }

    #[test]
    fn engine_section_parsed() {
        let toml = r#"
            [engine]
            target_fps = 120
            vsync = false
        "#;
        let cfg: EngineConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.engine.target_fps, Some(120));
        assert_eq!(cfg.engine.vsync, Some(false));
    }

    #[test]
    fn window_section_parsed() {
        let toml = r#"
            [window]
            title  = "My Game"
            width  = 1920
            height = 1080
            resizable = false
        "#;
        let cfg: EngineConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.window.title.as_deref(), Some("My Game"));
        assert_eq!(cfg.window.width, Some(1920));
        assert_eq!(cfg.window.height, Some(1080));
        assert_eq!(cfg.window.resizable, Some(false));
    }

    #[test]
    fn renderer_quality_parsed() {
        let toml = r#"
            [renderer]
            quality = "ultra"
            msaa = 4
        "#;
        let cfg: EngineConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.renderer.quality.as_deref(), Some("ultra"));
        assert_eq!(cfg.renderer.msaa, Some(4));
    }

    #[test]
    fn renderer_cel_sub_section_parsed() {
        let toml = r#"
            [renderer]
            style = "cel"
            [renderer.cel]
            toon_levels   = 3
            outline_width = 0.015
        "#;
        let cfg: EngineConfig = toml::from_str(toml).unwrap();
        let cel = cfg.renderer.cel.unwrap();
        assert_eq!(cel.toon_levels, Some(3));
        assert!((cel.outline_width.unwrap() - 0.015).abs() < 1e-6);
    }

    #[test]
    fn assets_section_parsed() {
        let toml = r#"
            [assets]
            hot_reload = true
        "#;
        let cfg: EngineConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.assets.hot_reload, Some(true));
    }

    // ── apply_to ──────────────────────────────────────────────────────────

    #[test]
    fn apply_to_sets_window_title() {
        let toml = r#"
            [window]
            title = "Overridden"
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.title, "Overridden");
    }

    #[test]
    fn apply_to_sets_resolution() {
        let toml = r#"
            [window]
            width = 2560
            height = 1440
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.width, 2560);
        assert_eq!(app_cfg.height, 1440);
    }

    #[test]
    fn apply_to_sets_vsync_false() {
        let toml = r#"
            [engine]
            vsync = false
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        assert!(app_cfg.vsync); // default is true
        engine_cfg.apply_to(&mut app_cfg);
        assert!(!app_cfg.vsync);
    }

    #[test]
    fn apply_to_target_fps_zero_means_unlimited() {
        let toml = r#"
            [engine]
            target_fps = 0
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert!(app_cfg.target_fps.is_none());
    }

    #[test]
    fn apply_to_quality_ultra_sets_msaa_4() {
        let toml = r#"
            [renderer]
            quality = "ultra"
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.render_quality, RenderQuality::Ultra);
        assert_eq!(app_cfg.sample_count, 4); // quality drives MSAA
    }

    #[test]
    fn apply_to_msaa_explicit_overrides_quality() {
        let toml = r#"
            [renderer]
            quality = "ultra"
            msaa = 1
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.render_quality, RenderQuality::Ultra);
        assert_eq!(app_cfg.sample_count, 1); // explicit wins
    }

    #[test]
    fn apply_to_style_flat() {
        let toml = r#"
            [renderer]
            style = "flat"
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.render_style, RenderStyle::FlatShaded);
    }

    #[test]
    fn apply_to_style_cel_with_params() {
        let toml = r#"
            [renderer]
            style = "cel"
            [renderer.cel]
            toon_levels   = 3
            outline_width = 0.02
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(
            app_cfg.render_style,
            RenderStyle::CelShaded { toon_levels: 3, outline_width: 0.02 }
        );
    }

    #[test]
    fn apply_to_hdri_path() {
        let toml = r#"
            [renderer]
            hdri = "assets/sky.exr"
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.hdri_path.as_deref(), Some("assets/sky.exr"));
    }

    #[test]
    fn apply_to_does_not_touch_unspecified_fields() {
        let toml = r#"
            [window]
            title = "X"
        "#;
        let engine_cfg: EngineConfig = toml::from_str(toml).unwrap();
        let mut app_cfg = default_app_config();
        let original_width = app_cfg.width;
        let original_fps = app_cfg.target_fps;
        engine_cfg.apply_to(&mut app_cfg);
        assert_eq!(app_cfg.width, original_width);
        assert_eq!(app_cfg.target_fps, original_fps);
    }

    // ── load_config: missing file ─────────────────────────────────────────

    #[test]
    fn load_config_missing_file_returns_default() {
        let cfg = load_config("__nonexistent_ferrous_test__.toml").unwrap();
        assert!(cfg.window.title.is_none());
    }

    // ── RenderQuality helpers ─────────────────────────────────────────────

    #[test]
    fn render_quality_default_is_high() {
        assert_eq!(RenderQuality::default(), RenderQuality::High);
    }

    #[test]
    fn render_quality_from_str_roundtrip() {
        for variant in [
            RenderQuality::Ultra,
            RenderQuality::High,
            RenderQuality::Medium,
            RenderQuality::Low,
            RenderQuality::Minimal,
        ] {
            let name = variant.as_str();
            assert_eq!(RenderQuality::from_str(name), Some(variant));
        }
    }

    #[test]
    fn render_quality_from_str_case_insensitive() {
        assert_eq!(RenderQuality::from_str("ULTRA"), Some(RenderQuality::Ultra));
        assert_eq!(RenderQuality::from_str("High"), Some(RenderQuality::High));
    }

    #[test]
    fn render_quality_from_str_unknown_returns_none() {
        assert_eq!(RenderQuality::from_str("fantastic"), None);
    }

    #[test]
    fn render_quality_ultra_flags() {
        assert!(RenderQuality::Ultra.ssao_enabled());
        assert!(RenderQuality::Ultra.bloom_enabled());
        assert!(RenderQuality::Ultra.shadows_enabled());
        assert!(RenderQuality::Ultra.ibl_enabled());
        assert_eq!(RenderQuality::Ultra.shadow_resolution(), 2048);
        assert_eq!(RenderQuality::Ultra.msaa_sample_count(), 4);
    }

    #[test]
    fn render_quality_low_flags() {
        assert!(!RenderQuality::Low.ssao_enabled());
        assert!(!RenderQuality::Low.bloom_enabled());
        assert!(!RenderQuality::Low.shadows_enabled());
        assert!(!RenderQuality::Low.ibl_enabled());
        assert_eq!(RenderQuality::Low.shadow_resolution(), 0);
        assert_eq!(RenderQuality::Low.msaa_sample_count(), 1);
    }

    #[test]
    fn render_quality_medium_has_no_ssao_but_has_shadows() {
        assert!(!RenderQuality::Medium.ssao_enabled());
        assert!(RenderQuality::Medium.bloom_enabled());
        assert!(RenderQuality::Medium.shadows_enabled());
        assert!(!RenderQuality::Medium.ibl_enabled());
        assert_eq!(RenderQuality::Medium.shadow_resolution(), 512);
    }
}
