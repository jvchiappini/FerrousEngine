//! The gallery CLI.
//!
//! ```text
//! cargo run -p guimaker_gallery -- shot <story> [--size 1280x800] [--scale 1.5]
//!     [--theme light|dark|high-contrast] [--out out/x.png] [--check golden.png]
//! cargo run -p guimaker_gallery -- golden <story> [--size ...] [--scale ...]
//!     [--theme ...] [--update --reason "..."] [--out goldens/x.png]
//! cargo run -p guimaker_gallery -- stories
//! ```

use std::path::{Path, PathBuf};

use guimaker::geom::Size;
use guimaker::theme::Theme;
use guimaker_gallery::golden::{append_update_log, compare_png, Tolerance};
use guimaker_gallery::shot::{render_story, write_png};
use guimaker_gallery::story::Story;
use guimaker_gallery::{GalleryError, Result};

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 800;
const DEFAULT_SCALE: f32 = 1.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        print_usage();
        return Ok(());
    };
    let rest: Vec<String> = args.collect();
    match command.as_str() {
        "shot" => command_shot(rest),
        "golden" => command_golden(rest),
        "stories" | "list" => {
            for story in Story::ALL {
                println!("{}", story.name());
            }
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => Err(GalleryError::Usage(format!(
            "unknown command {other:?}; try `shot`, `golden`, `stories` or `help`"
        ))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeKind {
    Light,
    Dark,
    HighContrast,
}

impl ThemeKind {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            "high-contrast" | "high_contrast" | "highcontrast" => Ok(Self::HighContrast),
            other => Err(GalleryError::Usage(format!(
                "unknown theme {other:?}; expected light, dark or high-contrast"
            ))),
        }
    }

    fn theme(self) -> Theme {
        match self {
            Self::Light => Theme::light(),
            Self::Dark => Theme::dark(),
            Self::HighContrast => Theme::high_contrast(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::HighContrast => "high-contrast",
        }
    }
}

#[derive(Clone, Debug)]
struct Options {
    story: Story,
    width: u32,
    height: u32,
    scale: f32,
    theme: ThemeKind,
    out: Option<PathBuf>,
    check: Option<PathBuf>,
    update: bool,
    reason: Option<String>,
}

impl Options {
    #[allow(clippy::cast_precision_loss)] // reason: window dimensions sit far below 2^24
    fn logical_size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    fn name(&self) -> String {
        format!(
            "{}-{}-{}x{}-{}",
            self.story.name(),
            self.theme.name(),
            self.width,
            self.height,
            scale_label(self.scale)
        )
    }

    fn default_shot(&self) -> PathBuf {
        PathBuf::from("out").join(format!("{}.png", self.name()))
    }

    fn default_golden(&self) -> PathBuf {
        goldens_directory().join(format!("{}.png", self.name()))
    }
}

fn parse_options(args: Vec<String>) -> Result<Options> {
    let mut args = args.into_iter();
    let Some(story) = args.next() else {
        return Err(GalleryError::Usage(
            "expected a story name (see `gallery stories`)".to_string(),
        ));
    };
    let story = Story::parse(&story).ok_or_else(|| {
        GalleryError::Usage(format!(
            "unknown story {story:?}; expected one of {}",
            story_names()
        ))
    })?;

    let mut options = Options {
        story,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        scale: DEFAULT_SCALE,
        theme: ThemeKind::Dark,
        out: None,
        check: None,
        update: false,
        reason: None,
    };

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--size" => {
                let value = value_after(&mut args, "--size")?;
                let (width, height) = parse_size(&value)?;
                options.width = width;
                options.height = height;
            }
            "--scale" => {
                let value = value_after(&mut args, "--scale")?;
                options.scale = value
                    .parse::<f32>()
                    .map_err(|_| GalleryError::Usage(format!("invalid --scale {value:?}")))?;
                if !(options.scale.is_finite() && options.scale > 0.0) {
                    return Err(GalleryError::Usage(format!(
                        "--scale must be finite and positive, got {}",
                        options.scale
                    )));
                }
            }
            "--theme" => {
                let value = value_after(&mut args, "--theme")?;
                options.theme = ThemeKind::parse(&value)?;
            }
            "--out" => {
                options.out = Some(PathBuf::from(value_after(&mut args, "--out")?));
            }
            "--check" => {
                options.check = Some(PathBuf::from(value_after(&mut args, "--check")?));
            }
            "--golden" => {
                options.out = Some(PathBuf::from(value_after(&mut args, "--golden")?));
            }
            "--update" => options.update = true,
            "--reason" => {
                options.reason = Some(value_after(&mut args, "--reason")?);
            }
            other => {
                return Err(GalleryError::Usage(format!(
                    "unknown option {other:?}; try `help`"
                )));
            }
        }
    }

    if options.update && options.reason.as_deref().is_none_or(str::is_empty) {
        return Err(GalleryError::Usage(
            "--update requires a non-empty --reason \"...\"".to_string(),
        ));
    }
    Ok(options)
}

fn command_shot(args: Vec<String>) -> Result<()> {
    let options = parse_options(args)?;
    if options.update {
        return Err(GalleryError::Usage(
            "`shot` does not rewrite goldens; use `golden ... --update --reason`".to_string(),
        ));
    }
    let image = render_story(
        options.story,
        &options.theme.theme(),
        options.logical_size(),
        options.scale,
    )?;

    if let Some(check) = &options.check {
        let tolerance = Tolerance::gallery(image.pixel_count());
        let report = compare_png(check, &image, tolerance)?;
        println!("check {} vs render: {}", check.display(), report.summary());
        if !report.within_tolerance {
            return Err(GalleryError::Golden(format!(
                "{} failed the tolerance: {}",
                check.display(),
                report.summary()
            )));
        }
    }

    let out = options
        .out
        .clone()
        .unwrap_or_else(|| options.default_shot());
    write_png(&out, &image)?;
    println!("{}", absolute(&out)?.display());
    println!(
        "{}x{} px, {} bytes RGBA, theme {}, scale {}",
        image.width,
        image.height,
        image.rgba.len(),
        options.theme.name(),
        scale_label(options.scale)
    );
    Ok(())
}

fn command_golden(args: Vec<String>) -> Result<()> {
    let options = parse_options(args)?;
    let image = render_story(
        options.story,
        &options.theme.theme(),
        options.logical_size(),
        options.scale,
    )?;

    let path = options
        .out
        .clone()
        .or_else(|| options.check.clone())
        .unwrap_or_else(|| options.default_golden());

    if options.update {
        let reason = options.reason.as_deref().unwrap_or_default();
        write_png(&path, &image)?;
        let log = append_update_log(&path, reason)?;
        println!("{}", absolute(&path)?.display());
        println!(
            "updated golden `{}`; reason recorded in {}",
            options.name(),
            absolute(&log)?.display()
        );
        return Ok(());
    }

    let tolerance = Tolerance::gallery(image.pixel_count());
    let report = compare_png(&path, &image, tolerance)?;
    println!("{}", absolute(&path)?.display());
    println!("{}", report.summary());
    if !report.within_tolerance {
        return Err(GalleryError::Golden(format!(
            "`{}` is out of tolerance: {}",
            options.name(),
            report.summary()
        )));
    }
    println!("golden OK");
    Ok(())
}

fn value_after(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| GalleryError::Usage(format!("{flag} needs a value")))
}

fn parse_size(value: &str) -> Result<(u32, u32)> {
    let (width, height) = value
        .split_once('x')
        .or_else(|| value.split_once('X'))
        .ok_or_else(|| {
            GalleryError::Usage(format!("invalid --size {value:?}; use WIDTHxHEIGHT"))
        })?;
    let width = width
        .parse::<u32>()
        .map_err(|_| GalleryError::Usage(format!("invalid width in --size {value:?}")))?;
    let height = height
        .parse::<u32>()
        .map_err(|_| GalleryError::Usage(format!("invalid height in --size {value:?}")))?;
    if width == 0 || height == 0 {
        return Err(GalleryError::Usage(format!(
            "--size must be positive, got {width}x{height}"
        )));
    }
    Ok((width, height))
}

fn scale_label(scale: f32) -> String {
    let mut text = format!("{scale}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

fn story_names() -> String {
    Story::ALL
        .iter()
        .map(|story| story.name())
        .collect::<Vec<_>>()
        .join(", ")
}

fn goldens_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens")
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::path::absolute(path).map_err(GalleryError::Io)
    }
}

fn print_usage() {
    println!(
        "guimaker_gallery - stories, headless screenshots and tolerance goldens\n\
         \n\
         USAGE:\n\
         \x20 gallery stories\n\
         \x20 gallery shot <story> [--size 1280x800] [--scale 1.5] [--theme light|dark|high-contrast]\n\
         \x20     [--out out/x.png] [--check golden.png]\n\
         \x20 gallery golden <story> [--size ...] [--scale ...] [--theme ...]\n\
         \x20     [--update --reason \"...\"] [--out goldens/x.png]\n\
         \n\
         Defaults: --size 1280x800 --scale 1 --theme dark\n\
         The PNG is physical: round(logical * scale) per axis."
    );
}
