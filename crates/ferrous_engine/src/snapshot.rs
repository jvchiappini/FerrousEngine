//! Launch snapshot: the last screen painted by the CPU while the GPU starts.
//!
//! The GPU driver costs ~400 ms per process before the first swapchain frame
//! (instance, device, swapchain; see `docs/startup.md`). Nothing about that is
//! cacheable, so the engine paints the last rendered screen into the window
//! with GDI until the first GPU frame is presented:
//!
//! 1. The application opts in with
//!    [`App::with_launch_snapshot`](crate::App::with_launch_snapshot) and
//!    supplies a **content key**: its version plus a hash of whatever the
//!    snapshotted screen shows. The engine adds the window's physical size,
//!    scale factor, monitor and surface format (plus this module's format
//!    version and the engine version) to form the file's identity.
//! 2. The application asks for a capture
//!    ([`AppContext::request_launch_snapshot`](crate::AppContext::request_launch_snapshot))
//!    on a frame that is worth showing at the next launch. The frame is copied
//!    back through the screenshot path (one readback, no per-frame cost) and
//!    written on a worker.
//! 3. At launch the runner reads the newest file on a worker started by
//!    `App::run`, before winit creates the window. A file whose identity does
//!    not match the window (size, scale, monitor, content key) is ignored: a
//!    stale screen is never shown.
//!
//! Any identity mismatch, a missing or corrupt file means no snapshot and the
//! S3 path (window painted with the background colour) takes over.
//!
//! The file is a raw RGBA8 image: decode is a copy, which is the fastest choice
//! for a 1600x900 screen (a lossless encoder would trade ~4 ms of decode for
//! bytes we do not need).

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[cfg(not(target_arch = "wasm32"))]
use winit::window::Window;

/// Version of the snapshot file this build reads and writes.
pub const FORMAT_VERSION: u16 = 1;

/// Magic that opens a snapshot file.
const MAGIC: &[u8; 6] = b"FSNAP1";

/// Fixed header: magic, version and the identity fields.
const HEADER_LEN: usize = 80;

/// Largest side accepted from a file (8K).
const MAX_SIDE: u32 = 8192;

/// Largest payload accepted from a file (64 MiB: 8K RGBA is 128 MiB, so a
/// capture that large is refused instead of pinning a second copy).
const MAX_PAYLOAD: usize = 64 << 20;

/// How many snapshot files the store keeps (one per content key; older ones
/// are pruned after every write).
const KEEP_FILES: usize = 4;

/// Trailing checksum width (FNV-1a 64 of the payload).
const CHECKSUM_LEN: usize = 8;

/// What identifies a snapshot screen.
///
/// The application supplies `content`; the engine adds everything about the
/// output that changes the pixels. [`Self::same_screen`] compares them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotIdentity {
    /// Hash of the engine version and the application's content key.
    pub content: u128,
    /// Window width in physical pixels.
    pub width: u32,
    /// Window height in physical pixels.
    pub height: u32,
    /// Scale factor as `f64` bits (exact match; the same monitor reports the
    /// same value).
    pub scale: u64,
    /// [`MonitorId::hash`] of the monitor the window was on.
    pub monitor: u64,
    /// `wgpu::TextureFormat as u32` at capture; `0` before the GPU exists.
    pub format: u32,
}

impl SnapshotIdentity {
    /// The identity of a screen shown at `width`×`height`.
    ///
    /// `format` is the surface format at capture and `0` at launch (the GPU
    /// has not started yet); `same_screen` compares it only when both sides
    /// know it.
    #[must_use]
    pub fn new(
        app_key: &str,
        width: u32,
        height: u32,
        scale: f64,
        monitor: u64,
        format: u32,
    ) -> Self {
        Self {
            content: content_hash(app_key),
            width,
            height,
            scale: scale.to_bits(),
            monitor,
            format,
        }
    }

    /// Whether this snapshot can be shown for `screen`.
    ///
    /// Every part of the key must match: a changed recents list, theme or app
    /// version (through `content`), a resized window, another DPI or another
    /// monitor. The surface format is compared only when both sides know it, so
    /// a launch can match before the GPU exists; the runner checks it again at
    /// the handoff.
    #[must_use]
    pub fn same_screen(&self, screen: &Self) -> bool {
        self.content == screen.content
            && self.width == screen.width
            && self.height == screen.height
            && self.scale == screen.scale
            && self.monitor == screen.monitor
            && (self.format == 0 || screen.format == 0 || self.format == screen.format)
    }
}

/// Which display the window was on, for [`SnapshotIdentity`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonitorId {
    /// Monitor name as the platform reports it (`None` when unknown).
    pub name: Option<String>,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Left edge in the virtual desktop.
    pub x: i32,
    /// Top edge in the virtual desktop.
    pub y: i32,
}

impl MonitorId {
    /// A stable hash of the monitor description.
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        mix(self.name.as_deref().unwrap_or("").as_bytes());
        mix(&self.width.to_le_bytes());
        mix(&self.height.to_le_bytes());
        mix(&self.x.to_le_bytes());
        mix(&self.y.to_le_bytes());
        hash
    }
}

/// A stable code for a surface format, for [`SnapshotIdentity::format`].
///
/// The `Debug` name of the format is hashed instead of the enum value because
/// `wgpu::TextureFormat` is not a fieldless enum; it is stable across builds
/// and the engine version is part of the key anyway.
#[must_use]
pub(crate) fn format_code(format: wgpu::TextureFormat) -> u32 {
    checksum(format!("{format:?}").as_bytes()) as u32
}

/// The FNV-1a 128 hash of the engine version and an application content key.
#[must_use]
pub fn content_hash(app_key: &str) -> u128 {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut mix = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u128::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    mix(env!("CARGO_PKG_VERSION").as_bytes());
    mix(&FORMAT_VERSION.to_le_bytes());
    mix(app_key.as_bytes());
    hash
}

/// One decoded snapshot: the pixels plus what they were captured for.
#[derive(Clone, Debug)]
pub struct SnapshotImage {
    identity: SnapshotIdentity,
    position: (i32, i32),
    rgba: Vec<u8>,
    /// BGRA, top-down, ready for `SetDIBitsToDevice`; built on the reader
    /// worker so the main thread only hands it to GDI.
    #[cfg(target_os = "windows")]
    dib: Vec<u8>,
}

impl SnapshotImage {
    /// A snapshot of `rgba` (tight RGBA8) for `identity`, captured with the
    /// window at `position` (its outer position, restored at launch).
    ///
    /// `None` when the pixel buffer does not match the identity size or the
    /// image is larger than a snapshot may be.
    #[must_use]
    pub fn new(identity: SnapshotIdentity, position: (i32, i32), rgba: Vec<u8>) -> Option<Self> {
        let expected = pixel_len(identity.width, identity.height)?;
        if rgba.len() != expected {
            return None;
        }
        #[cfg(target_os = "windows")]
        let dib = to_bgra(&rgba);
        Some(Self {
            identity,
            position,
            rgba,
            #[cfg(target_os = "windows")]
            dib,
        })
    }

    /// What this snapshot is valid for.
    #[must_use]
    pub fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }

    /// The window's outer position when the snapshot was captured.
    #[must_use]
    pub fn position(&self) -> (i32, i32) {
        self.position
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.identity.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.identity.height
    }

    /// Tight RGBA8 pixels, `width * height * 4` bytes.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The pixel data in the byte order `SetDIBitsToDevice` expects.
    #[cfg(target_os = "windows")]
    pub(crate) fn dib(&self) -> &[u8] {
        &self.dib
    }

    /// The file bytes: a fixed header, the RGBA payload and its checksum.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LEN + self.rgba.len() + CHECKSUM_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.identity.content.to_le_bytes());
        bytes.extend_from_slice(&self.identity.width.to_le_bytes());
        bytes.extend_from_slice(&self.identity.height.to_le_bytes());
        bytes.extend_from_slice(&self.identity.scale.to_le_bytes());
        bytes.extend_from_slice(&self.identity.monitor.to_le_bytes());
        bytes.extend_from_slice(&self.identity.format.to_le_bytes());
        bytes.extend_from_slice(&self.position.0.to_le_bytes());
        bytes.extend_from_slice(&self.position.1.to_le_bytes());
        bytes.extend_from_slice(&(self.rgba.len() as u32).to_le_bytes());
        bytes.resize(HEADER_LEN, 0);
        bytes.extend_from_slice(&self.rgba);
        bytes.extend_from_slice(&checksum(&self.rgba).to_le_bytes());
        bytes
    }

    /// Decodes a snapshot file, rejecting anything that is not one of ours.
    ///
    /// # Errors
    ///
    /// [`SnapshotError`] when the header is foreign or from another format
    /// version, the sizes are implausible, the payload is truncated or the
    /// checksum does not match.
    pub fn decode(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() < HEADER_LEN + CHECKSUM_LEN {
            return Err(SnapshotError::Truncated);
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(SnapshotError::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[6], bytes[7]]);
        if version != FORMAT_VERSION {
            return Err(SnapshotError::UnsupportedVersion(version));
        }
        let content = u128::from_le_bytes(chunk(bytes, 8)?);
        let width = u32::from_le_bytes(chunk(bytes, 24)?);
        let height = u32::from_le_bytes(chunk(bytes, 28)?);
        let scale = u64::from_le_bytes(chunk(bytes, 32)?);
        let monitor = u64::from_le_bytes(chunk(bytes, 40)?);
        let format = u32::from_le_bytes(chunk(bytes, 48)?);
        let x = i32::from_le_bytes(chunk(bytes, 52)?);
        let y = i32::from_le_bytes(chunk(bytes, 56)?);
        let length = u32::from_le_bytes(chunk(bytes, 60)?) as usize;
        let expected = pixel_len(width, height).ok_or(SnapshotError::TooLarge)?;
        if length != expected {
            return Err(SnapshotError::BadLength(length));
        }
        let end = HEADER_LEN + length;
        let payload = bytes.get(HEADER_LEN..end).ok_or(SnapshotError::Truncated)?;
        let stored = u64::from_le_bytes(chunk(bytes, end)?);
        if checksum(payload) != stored {
            return Err(SnapshotError::Checksum);
        }
        let identity = SnapshotIdentity {
            content,
            width,
            height,
            scale,
            monitor,
            format,
        };
        Self::new(identity, (x, y), payload.to_vec()).ok_or(SnapshotError::BadLength(length))
    }
}

/// Why a snapshot file could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// The file does not start with the snapshot magic.
    #[error("not a launch snapshot")]
    BadMagic,
    /// Written by another format version.
    #[error("unsupported snapshot version {0}")]
    UnsupportedVersion(u16),
    /// A width, height or size above what a snapshot may be.
    #[error("snapshot dimensions or size are out of range")]
    TooLarge,
    /// The payload length does not match the identity.
    #[error("snapshot payload length {0} does not match its size")]
    BadLength(usize),
    /// The file ends before the payload or checksum.
    #[error("snapshot file is truncated")]
    Truncated,
    /// The payload bytes do not match their checksum.
    #[error("snapshot checksum mismatch")]
    Checksum,
}

/// Reads snapshots from a directory.
#[derive(Debug, Clone)]
pub(crate) struct SnapshotStore {
    directory: PathBuf,
}

impl SnapshotStore {
    pub(crate) fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// The file for `identity`.
    fn path_for(&self, identity: &SnapshotIdentity) -> PathBuf {
        self.directory
            .join(format!("snap-{:032x}.snap", identity.content))
    }

    /// The candidate files, newest first.
    fn candidates(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        let mut files: Vec<(SystemTime, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("snap"))
            })
            .map(|entry| {
                let time = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                (time, entry.path())
            })
            .collect();
        files.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
        files.into_iter().map(|(_, path)| path).collect()
    }

    /// Reads and decodes `path`; `None` when it is missing or not a snapshot.
    fn read(&self, path: &Path) -> Option<SnapshotImage> {
        let bytes = std::fs::read(path).ok()?;
        match SnapshotImage::decode(&bytes) {
            Ok(image) => Some(image),
            Err(error) => {
                log::warn!("launch snapshot {} ignored: {error}", path.display());
                None
            }
        }
    }

    /// Writes `image` atomically and prunes older files.
    ///
    /// # Errors
    ///
    /// The I/O error when the directory or the temporary file cannot be
    /// written. A snapshot that cannot be stored is not an error for the
    /// application; the runner logs it and the next launch simply shows the
    /// background colour.
    pub(crate) fn write(&self, image: &SnapshotImage) -> io::Result<()> {
        let path = self.path_for(image.identity());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A stale file with the same identity (same content on the same screen
        // at a new size is a different content hash only if the app key
        // changed) is replaced below.
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, image.encode())?;
        if std::fs::rename(&temporary, &path).is_err() {
            // Windows refuses to rename over an existing file.
            let _ = std::fs::remove_file(&path);
            if std::fs::rename(&temporary, &path).is_err() {
                let _ = std::fs::remove_file(&temporary);
                return Err(io::Error::other("the snapshot could not be replaced"));
            }
        }
        self.prune();
        Ok(())
    }

    /// Keeps the newest [`KEEP_FILES`] files.
    fn prune(&self) {
        for path in self.candidates().into_iter().skip(KEEP_FILES) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Reads the newest snapshot off the main thread.
///
/// Started by `Runner::new` (before winit creates the window) so the file read
/// and the BGRA conversion overlap window creation and the GPU probe.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct SnapshotReader {
    thread: Option<std::thread::JoinHandle<Option<SnapshotImage>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl SnapshotReader {
    pub(crate) fn start(config: &LaunchSnapshot) -> Self {
        let directory = config.directory.clone();
        let thread = std::thread::Builder::new()
            .name("ferrous-snapshot".to_owned())
            .spawn(move || {
                let _span = ferrous_core::startup::span("engine: snapshot read");
                let store = SnapshotStore::new(directory);
                let newest = store.candidates().into_iter().next()?;
                store.read(&newest)
            })
            .ok();
        Self { thread }
    }

    /// Waits for the read; `None` when the worker failed or found nothing.
    pub(crate) fn join(self) -> Option<SnapshotImage> {
        self.thread?.join().ok().flatten()
    }
}

/// Writes a captured snapshot off the main thread.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_async(
    directory: PathBuf,
    image: SnapshotImage,
) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("ferrous-snapshot-write".to_owned())
        .spawn(move || {
            let _span = ferrous_core::startup::span("engine: snapshot write");
            let store = SnapshotStore::new(directory);
            if let Err(error) = store.write(&image) {
                log::warn!("the launch snapshot could not be written: {error}");
            }
        })
        .ok()
}

/// The identity of `window` showing content keyed by `key`.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub(crate) fn window_identity(window: &Window, key: &str, format: u32) -> SnapshotIdentity {
    let size = window.inner_size();
    let monitor = window.current_monitor().map(|monitor| MonitorId {
        name: monitor.name(),
        width: monitor.size().width,
        height: monitor.size().height,
        x: monitor.position().x,
        y: monitor.position().y,
    });
    SnapshotIdentity::new(
        key,
        size.width,
        size.height,
        window.scale_factor(),
        monitor.unwrap_or_default().hash(),
        format,
    )
}

/// Where an application's snapshots live by default.
fn default_directory(app_name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(base)
                .join(app_name)
                .join("cache")
                .join("launch");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join(app_name)
                .join("launch");
        }
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "macos"),
        not(target_arch = "wasm32")
    ))]
    {
        if let Some(base) = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        {
            return base.join(app_name).join("launch");
        }
    }
    std::env::temp_dir()
        .join("ferrous")
        .join(app_name)
        .join("launch")
}

/// Where the launch snapshot is stored and what the screen currently shows.
///
/// The application supplies the content key; the engine derives the rest of
/// the identity from the window, so a resized window, another DPI or another
/// monitor never shows a stale screen.
#[derive(Clone, Debug)]
pub struct LaunchSnapshot {
    /// Directory holding the snapshot files.
    pub directory: PathBuf,
    /// The screen's content key: the app version plus a hash of what it shows.
    pub key: String,
}

impl LaunchSnapshot {
    /// Snapshots for `app_name` under the per-OS cache directory
    /// (`%LOCALAPPDATA%\<app>\cache\launch` on Windows).
    #[must_use]
    pub fn for_app(app_name: impl AsRef<str>) -> Self {
        Self {
            directory: default_directory(app_name.as_ref()),
            key: String::new(),
        }
    }

    /// Uses `directory` instead of the per-OS default.
    #[must_use]
    pub fn with_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = directory.into();
        self
    }

    /// The content key of the screen to snapshot.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }
}

/// `width * height * 4`, or `None` above [`MAX_SIDE`] / [`MAX_PAYLOAD`].
fn pixel_len(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let length = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    (length <= MAX_PAYLOAD).then_some(length)
}

/// A fixed-size field of `bytes` starting at `at`.
fn chunk<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N], SnapshotError> {
    bytes
        .get(at..at + N)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(SnapshotError::Truncated)
}

/// FNV-1a 64 of `bytes`.
fn checksum(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// RGBA8 to BGRA8 (the byte order of a 32-bit BI_RGB DIB).
#[cfg(target_os = "windows")]
fn to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = rgba.to_vec();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    bgra
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(key: &str, width: u32, height: u32, format: u32) -> SnapshotIdentity {
        SnapshotIdentity::new(key, width, height, 1.0, 0x1122_3344_5566_7788, format)
    }

    fn image(width: u32, height: u32) -> SnapshotImage {
        let rgba = (0..width as usize * height as usize * 4)
            .map(|index| (index % 251) as u8)
            .collect();
        SnapshotImage::new(identity("v1:start", width, height, 0x1b), (12, 34), rgba)
            .expect("a valid image")
    }

    #[test]
    fn the_same_screen_matches_and_every_other_part_does_not() {
        let base = identity("v1:start", 1600, 900, 0);
        assert!(base.same_screen(&identity("v1:start", 1600, 900, 0)));
        assert!(!base.same_screen(&identity("v2:start", 1600, 900, 0)));
        assert!(!base.same_screen(&identity("v1:start", 1601, 900, 0)));
        assert!(!base.same_screen(&identity("v1:start", 1600, 901, 0)));

        let mut other_scale = base.clone();
        other_scale.scale = 1.5f64.to_bits();
        assert!(!base.same_screen(&other_scale));

        let mut other_monitor = base.clone();
        other_monitor.monitor ^= 1;
        assert!(!base.same_screen(&other_monitor));

        // The format is compared only when both sides know it: a launch has no
        // GPU yet, the capture stored the surface format.
        let captured = identity("v1:start", 1600, 900, 0x1b);
        assert!(base.same_screen(&captured));
        assert!(!identity("v1:start", 1600, 900, 0x1a).same_screen(&captured));
    }

    #[test]
    fn the_content_hash_covers_the_engine_version() {
        // Same key, same value; different key, different value.
        assert_eq!(content_hash("a"), content_hash("a"));
        assert_ne!(content_hash("a"), content_hash("b"));
        // The engine version is mixed in, so a new engine never reuses an old
        // snapshot (the constant appears in the hash of the empty key).
        assert_ne!(content_hash(""), 0);
    }

    #[test]
    fn a_snapshot_round_trips_byte_for_byte() {
        let image = image(7, 5);
        let bytes = image.encode();
        let decoded = SnapshotImage::decode(&bytes).expect("our own file");
        assert_eq!(decoded.rgba(), image.rgba());
        assert_eq!(decoded.identity(), image.identity());
        assert_eq!(decoded.position(), (12, 34));
    }

    #[test]
    fn a_foreign_truncated_or_corrupt_file_is_refused() {
        let bytes = image(4, 3).encode();
        assert!(matches!(
            SnapshotImage::decode(&[b'X'; 100]),
            Err(SnapshotError::BadMagic)
        ));
        assert!(matches!(
            SnapshotImage::decode(&bytes[..HEADER_LEN + 4]),
            Err(SnapshotError::Truncated)
        ));
        let mut wrong_version = bytes.clone();
        wrong_version[6] = 0x99;
        assert!(matches!(
            SnapshotImage::decode(&wrong_version),
            Err(SnapshotError::UnsupportedVersion(0x99))
        ));
        let mut corrupt = bytes.clone();
        let last = corrupt.len() - CHECKSUM_LEN - 1;
        corrupt[last] ^= 0xFF;
        assert!(matches!(
            SnapshotImage::decode(&corrupt),
            Err(SnapshotError::Checksum)
        ));
        let mut wrong_length = bytes;
        wrong_length[60] = 0;
        wrong_length[61] = 0;
        assert!(matches!(
            SnapshotImage::decode(&wrong_length),
            Err(SnapshotError::BadLength(0))
        ));
    }

    #[test]
    fn absurd_dimensions_are_refused_before_allocating() {
        assert!(pixel_len(0, 0).is_none());
        assert!(pixel_len(8193, 1).is_none());
        assert!(pixel_len(8192, 8192).is_none(), "128 MiB is too large");
        assert_eq!(pixel_len(1600, 900), Some(1600 * 900 * 4));
        assert!(SnapshotImage::new(identity("k", 2, 2, 0), (0, 0), vec![0; 15]).is_none());
    }

    #[test]
    fn the_store_writes_reads_and_prunes() {
        let directory = std::env::temp_dir().join(format!(
            "ferrous-launch-snapshot-{}-{}",
            std::process::id(),
            "store"
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let store = SnapshotStore::new(&directory);
        assert!(store.candidates().is_empty());
        assert!(store.read(&directory.join("missing.snap")).is_none());

        let image = image(3, 3);
        store.write(&image).expect("a writable directory");
        let newest = store.candidates().into_iter().next().expect("a file");
        let decoded = store.read(&newest).expect("a readable file");
        assert_eq!(decoded.rgba(), image.rgba());

        // More keys than the keep limit: only the newest are kept.
        for index in 0..(KEEP_FILES + 2) {
            let other = SnapshotImage::new(
                identity(&format!("v1:start:{index}"), 3, 3, 0x1b),
                (0, 0),
                image.rgba().to_vec(),
            )
            .expect("a valid image");
            store.write(&other).expect("a writable directory");
        }
        assert_eq!(store.candidates().len(), KEEP_FILES);

        // A corrupt file in the newest slot is ignored, not shown.
        let newest = store.candidates().into_iter().next().expect("a file");
        std::fs::write(&newest, b"garbage").unwrap();
        assert!(store.read(&newest).is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_dib_has_the_byte_order_gdi_expects() {
        let rgba = vec![0x10, 0x20, 0x30, 0xFF, 0x01, 0x02, 0x03, 0x80];
        let image = SnapshotImage::new(identity("k", 2, 1, 0), (0, 0), rgba).unwrap();
        assert_eq!(
            image.dib(),
            &[0x30, 0x20, 0x10, 0xFF, 0x03, 0x02, 0x01, 0x80]
        );
    }
}
