//! The gallery CLI.
//!
//! ```text
//! cargo run -p guimaker_gallery -- shot <story> [--size 1280x800] [--scale 1.5]
//!     [--theme light|dark|high-contrast] [--out out/x.png] [--check golden.png]
//! cargo run -p guimaker_gallery -- golden <story> [--size ...] [--scale ...]
//!     [--theme ...] [--update --reason "..."] [--out goldens/x.png]
//! cargo run -p guimaker_gallery -- lint <story> [--sweep] [--size ...]
//!     [--scale ...] [--theme ...] [--json]
//! cargo run -p guimaker_gallery -- stories
//! ```

use std::path::{Path, PathBuf};

use guimaker::geom::Size;
use guimaker::lint::{Finding, Severity};
use guimaker::sweep::{sweep, Sweep};
use guimaker::text::MetricsTextSystem;
use guimaker::theme::Theme;
use guimaker::view::Element;
use guimaker::{Frame, Ui};
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
        "lint" => command_lint(rest),
        "shots" => command_shots(rest),
        "goldens" => command_goldens(rest),
        "stories" | "list" => {
            for story in Story::all() {
                println!("{}", story.name());
            }
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => Err(GalleryError::Usage(format!(
            "unknown command {other:?}; try `shot`, `golden`, `lint`, `stories` or `help`"
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
    sweep: bool,
    json: bool,
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
        sweep: false,
        json: false,
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
            "--sweep" => options.sweep = true,
            "--json" => options.json = true,
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

/// Paints `view` into a fresh `Ui` with the GPU-free metrics.
fn paint_view(view: &Element<()>, theme: &Theme, viewport: Size, scale: f32) -> Ui<()> {
    let mut ui: Ui<()> = Ui::new(theme.clone());
    ui.set_viewport(viewport, scale);
    let _ = ui.set_time_ms(0.0);
    let mut frame = Frame::new();
    let mut text = MetricsTextSystem::new();
    ui.paint(view, &mut frame, &mut text);
    ui
}

/// `gallery lint <story> [--sweep] [--size ...] [--scale ...] [--theme ...] [--json]`.
fn command_lint(args: Vec<String>) -> Result<()> {
    let options = parse_options(args)?;
    let theme = options.theme.theme();
    let view = options.story.view();
    if options.sweep {
        return lint_sweep(&view, &theme, &options);
    }
    let logical = options.logical_size();
    let ui = paint_view(&view, &theme, logical, options.scale);
    let findings = guimaker::lint::lint(&ui);
    if options.json {
        for finding in &findings {
            print_json_line(options.story, logical.width, options.scale, finding);
        }
    } else {
        print_report(options.story, &options, &findings);
    }
    let blocking = blocking_count(&findings);
    if blocking > 0 {
        return Err(GalleryError::Lint(format!(
            "{blocking} blocking finding(s) in `{}`",
            options.story.name()
        )));
    }
    Ok(())
}

/// Runs the responsive sweep and reports the lint's per-case findings.
fn lint_sweep(view: &Element<()>, theme: &Theme, options: &Options) -> Result<()> {
    let report = sweep(view, theme);
    if options.json {
        for case in &report.cases {
            for finding in &case.lint {
                print_json_line(options.story, case.width, case.scale, finding);
            }
        }
    } else {
        print_sweep_report(options.story, options, &report);
    }
    let blocking = report
        .lint_findings()
        .iter()
        .filter(|finding| finding.severity.is_blocking())
        .count();
    if blocking > 0 {
        return Err(GalleryError::Lint(format!(
            "{blocking} blocking finding(s) in the sweep of `{}`",
            options.story.name()
        )));
    }
    Ok(())
}

/// Prints a single-view lint report grouped by severity.
fn print_report(story: Story, options: &Options, findings: &[Finding]) {
    println!(
        "lint: {} ({}, {}x{} @ {}x)",
        story.name(),
        options.theme.name(),
        options.width,
        options.height,
        scale_label(options.scale)
    );
    if findings.is_empty() {
        println!("no findings");
        return;
    }
    let mut ordered: Vec<&Finding> = findings.iter().collect();
    ordered.sort_by_key(|finding| severity_rank(&finding.severity));
    let mut shown: Option<u8> = None;
    for finding in ordered {
        let rank = severity_rank(&finding.severity);
        if shown != Some(rank) {
            let count = findings
                .iter()
                .filter(|candidate| severity_rank(&candidate.severity) == rank)
                .count();
            println!("{} ({count}):", severity_label(&finding.severity));
            shown = Some(rank);
        }
        println!("  - {}", finding.sentence());
    }
    println!(
        "{} finding(s), {} blocking",
        findings.len(),
        blocking_count(findings)
    );
}

/// Prints the per-case findings and the first width of each rule/path.
fn print_sweep_report(story: Story, options: &Options, report: &Sweep) {
    println!(
        "sweep: {} ({}, {} case(s))",
        story.name(),
        options.theme.name(),
        report.cases.len()
    );
    for case in &report.cases {
        if case.lint.is_empty() {
            continue;
        }
        println!("{} @ {}x:", case.width, scale_label(case.scale));
        for finding in &case.lint {
            println!(
                "  [{}] {}",
                severity_label(&finding.severity),
                finding.sentence()
            );
        }
    }
    println!("first widths:");
    let first = report.first_widths();
    if first.is_empty() {
        println!("  none");
    }
    for entry in first {
        println!(
            "  {} `{}`: first at {} px",
            entry.rule, entry.path, entry.width
        );
    }
    println!(
        "{} finding(s) in {} case(s); {} blocking",
        report.lint_count(),
        report.cases.len(),
        report
            .lint_findings()
            .iter()
            .filter(|finding| finding.severity.is_blocking())
            .count()
    );
    println!("{}", report.summary());
}

fn blocking_count(findings: &[Finding]) -> usize {
    findings
        .iter()
        .filter(|finding| finding.severity.is_blocking())
        .count()
}

#[allow(clippy::trivially_copy_pass_by_ref)] // reason: severities are read from finding fields behind a shared reference
fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // reason: severities are read from finding fields behind a shared reference
fn severity_label(severity: &Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn print_json_line(story: Story, width: f32, scale: f32, finding: &Finding) {
    let rule = format!("{:?}", finding.rule);
    println!(
        "{{\"story\":\"{}\",\"width\":{width},\"scale\":{scale},\"severity\":\"{}\",\"rule\":\"{}\",\"path\":\"{}\",\"sentence\":\"{}\"}}",
        json_escape(story.name()),
        severity_label(&finding.severity),
        json_escape(&rule),
        json_escape(&finding.path),
        json_escape(&finding.sentence())
    );
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[derive(Clone, Debug)]
struct BatchOptions {
    width: u32,
    height: u32,
    scale: f32,
    out: PathBuf,
    reason: Option<String>,
}

fn parse_batch(args: Vec<String>) -> Result<BatchOptions> {
    let mut options = BatchOptions {
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        scale: 1.5,
        out: PathBuf::from("out/shots"),
        reason: None,
    };
    let mut args = args.into_iter();
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
            "--out" => {
                options.out = PathBuf::from(value_after(&mut args, "--out")?);
            }
            "--reason" => {
                options.reason = Some(value_after(&mut args, "--reason")?);
            }
            other => {
                return Err(GalleryError::Usage(format!(
                    "unknown option {other:?} for a batch command"
                )));
            }
        }
    }
    Ok(options)
}

#[allow(clippy::cast_precision_loss)] // reason: window dimensions sit far below 2^24
fn batch_logical(options: &BatchOptions) -> Size {
    Size::new(options.width as f32, options.height as f32)
}

fn batch_name(story: Story, theme: &str, options: &BatchOptions) -> String {
    format!(
        "{}-{}-{}x{}-{}",
        story.name(),
        theme,
        options.width,
        options.height,
        scale_label(options.scale)
    )
}

fn command_shots(args: Vec<String>) -> Result<()> {
    let options = parse_batch(args)?;
    let logical = batch_logical(&options);
    std::fs::create_dir_all(&options.out)?;
    for story in Story::all() {
        for (theme_name, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
            let image = render_story(story, &theme, logical, options.scale)?;
            let path = options
                .out
                .join(format!("{}.png", batch_name(story, theme_name, &options)));
            write_png(&path, &image)?;
            println!("{}", absolute(&path)?.display());
        }
    }
    Ok(())
}

fn command_goldens(args: Vec<String>) -> Result<()> {
    let options = parse_batch(args)?;
    let reason = options.reason.as_deref().unwrap_or_default();
    if reason.is_empty() {
        return Err(GalleryError::Usage(
            "`goldens` requires a non-empty --reason \"...\"".to_string(),
        ));
    }
    let logical = batch_logical(&options);
    let directory = goldens_directory();
    std::fs::create_dir_all(&directory)?;
    for story in Story::all() {
        for (theme_name, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
            let image = render_story(story, &theme, logical, options.scale)?;
            let path = directory.join(format!("{}.png", batch_name(story, theme_name, &options)));
            write_png(&path, &image)?;
            append_update_log(&path, reason)?;
            println!("{}", absolute(&path)?.display());
        }
    }
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
    Story::all()
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
         \x20 gallery lint <story> [--sweep] [--size ...] [--scale ...] [--theme ...] [--json]\n\
         \x20 gallery shots [--size 1280x800] [--scale 1.5] [--out out/shots]\n\
         \x20 gallery goldens [--reason \"...\"] [--size ...] [--scale ...]\n\
         \n\
         Defaults: --size 1280x800 --scale 1 --theme dark\n\
         The PNG is physical: round(logical * scale) per axis."
    );
}
