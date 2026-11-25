//! Game-mode branding: the official Roves logo (repository root `icon.png`, embedded at
//! build time) decoded and downscaled by SDL3 itself (`SDL_LoadPNG_IO`, SDL's bundled
//! stb_image, and `SDL_ScaleSurface`), no extra image crate. It becomes the window icon
//! (`SDL_SetWindowIcon`, unless the game ships its own icon, below) and the logo texture of
//! the dark startup/failure screen. The "Roves" wordmark under it is the official MetalMania
//! font rasterized at build time (`build.rs`, ab_glyph) into an embedded coverage bitmap, and
//! the indeterminate loading bar is two flat colours. The existing wgpu blit composes and
//! presents all of them (README "Packaged player").
//!
//! The window icon follows the project icon contract (README "Project icon"): a valid
//! `icon.png` next to `play.exe` (the game's own icon, copied there by the packaging tools)
//! replaces the Roves logo in the title bar and taskbar; the startup and failure screens
//! keep the Roves logo either way.

use std::path::Path;

use sdl3::{iostream::IOStream, pixels::PixelFormat, surface::Surface};

const ICON_PNG: &[u8] = include_bytes!("../../../icon.png");
/// Logo edge in physical pixels: the 1024 px source halved twice (each halving with linear
/// filtering averages exactly 2x2 texels).
pub(crate) const LOGO_PX: u32 = 256;
/// Dark startup-screen background (#141418), also the clear colour around the game frame
/// in game mode. The composed target is `Rgba8Unorm`, so these are the stored bytes / 255.
pub(crate) const SPLASH_BACKGROUND: [f32; 4] = [20.0 / 255.0, 20.0 / 255.0, 24.0 / 255.0, 1.0];
/// Opacity of the logo and wordmark on the failure screen (out of 255).
const FAILED_LOGO_ALPHA: u32 = 80;

// The "Roves" wordmark in the official MetalMania font, rasterized by build.rs into 8-bit
// coverage (`WORDMARK_SIZE`, row-major); no font or rasterizer at runtime.
include!(concat!(env!("OUT_DIR"), "/wordmark.rs"));
const WORDMARK_ALPHA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wordmark.alpha"));
/// Wordmark colour, the near-white of the logo (#F2F2F5).
const WORDMARK_COLOR: [u8; 3] = [242, 242, 245];
/// Loading-bar size in physical pixels, as wide as the wordmark.
pub(crate) const BAR_SIZE: (u32, u32) = (WORDMARK_SIZE.0, 6);
/// Loading-bar track (#34343C) and moving segment (#F2F2F5), opaque premultiplied RGBA8.
pub(crate) const BAR_TRACK: [u8; 4] = [52, 52, 60, 255];
pub(crate) const BAR_INDICATOR: [u8; 4] = [242, 242, 245, 255];
/// Vertical gap between the logo, the wordmark and the bar, in physical pixels.
pub(crate) const SPLASH_GAP: u32 = 24;

pub(crate) struct Branding {
    /// `LOGO_PX` square, straight alpha: `SDL_SetWindowIcon` input.
    pub(crate) icon: Surface<'static>,
    /// Tightly packed premultiplied RGBA8 of `LOGO_PX` x `LOGO_PX` (startup screen).
    pub(crate) logo: Vec<u8>,
    /// The same logo at `FAILED_LOGO_ALPHA` opacity (failure screen).
    pub(crate) failed: Vec<u8>,
}

/// The "Roves" wordmark, `WORDMARK_SIZE`, tightly packed premultiplied RGBA8.
pub(crate) struct Wordmark {
    pub(crate) rgba: Vec<u8>,
    /// The same at `FAILED_LOGO_ALPHA` opacity (failure screen).
    pub(crate) failed: Vec<u8>,
}

pub(crate) fn decode() -> Result<Branding, String> {
    let mut stream = IOStream::from_bytes(ICON_PNG).map_err(|e| format!("icon stream: {e}"))?;
    let mut surface = Surface::load_png_io(&mut stream)
        .and_then(|s| s.convert_format(PixelFormat::RGBA32))
        .map_err(|e| format!("icon.png decode: {e}"))?;
    if surface.width() != surface.height() || surface.width() < LOGO_PX {
        return Err(format!(
            "icon.png is {}x{}, expected a square of at least {LOGO_PX} px",
            surface.width(),
            surface.height()
        ));
    }
    for edge in crate::icon_rules::downscale_steps(surface.width()) {
        surface = scale(&surface, edge)?;
    }
    let icon = surface
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| format!("icon copy: {e}"))?;
    let logo = premultiplied_rgba(surface)?;
    let failed = dimmed(&logo);
    Ok(Branding { icon, logo, failed })
}

/// Where the window icon comes from.
pub(crate) enum ProjectIcon {
    /// A valid project `icon.png`, downscaled to at most `WINDOW_ICON_PX`, straight alpha.
    Found {
        icon: Surface<'static>,
        source_px: u32,
    },
    /// No `icon.png` next to the executable: the Roves logo stays.
    Absent,
    /// An `icon.png` that breaks the contract: the Roves logo stays, the reason is logged.
    Invalid(String),
}

/// The project icon next to `exe` (`play.exe`), decoded by SDL3 like the Roves logo after
/// the header checks of `icon_rules` (size, PNG signature, square bounds).
pub(crate) fn project_icon(exe: Option<&Path>) -> ProjectIcon {
    use crate::icon_rules::{MAX_ICON_BYTES, PROJECT_ICON_FILE, check_png, downscale_steps};
    let Some(path) = exe
        .and_then(Path::parent)
        .map(|dir| dir.join(PROJECT_ICON_FILE))
    else {
        return ProjectIcon::Absent;
    };
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return ProjectIcon::Absent,
        Err(error) => return ProjectIcon::Invalid(format!("metadata: {error}")),
    };
    if !metadata.is_file() {
        return ProjectIcon::Invalid("not a regular file".into());
    }
    if metadata.len() > MAX_ICON_BYTES {
        return ProjectIcon::Invalid(format!(
            "{} bytes exceeds the {MAX_ICON_BYTES}-byte limit",
            metadata.len()
        ));
    }
    let decoded = std::fs::read(&path)
        .map_err(|e| format!("read: {e}"))
        .and_then(|bytes| {
            let (edge, _) = check_png(&bytes)?;
            let mut stream = IOStream::from_bytes(&bytes).map_err(|e| format!("stream: {e}"))?;
            let mut surface = Surface::load_png_io(&mut stream)
                .and_then(|s| s.convert_format(PixelFormat::RGBA32))
                .map_err(|e| format!("decode: {e}"))?;
            if (surface.width(), surface.height()) != (edge, edge) {
                return Err(format!(
                    "decoded {}x{} differs from the {edge}x{edge} header",
                    surface.width(),
                    surface.height()
                ));
            }
            for step in downscale_steps(edge) {
                surface = scale(&surface, step)?;
            }
            Ok((surface, edge))
        });
    match decoded {
        Ok((icon, source_px)) => ProjectIcon::Found { icon, source_px },
        Err(error) => ProjectIcon::Invalid(error),
    }
}

/// The build-time MetalMania coverage tinted `WORDMARK_COLOR`, premultiplied RGBA8.
pub(crate) fn wordmark() -> Result<Wordmark, String> {
    let (width, height) = WORDMARK_SIZE;
    if WORDMARK_ALPHA.len() != (width * height) as usize {
        return Err(format!(
            "wordmark bitmap is {} bytes, expected {width}x{height}",
            WORDMARK_ALPHA.len()
        ));
    }
    let rgba: Vec<u8> = WORDMARK_ALPHA
        .iter()
        .flat_map(|&a| {
            let [r, g, b] =
                WORDMARK_COLOR.map(|c| ((u32::from(c) * u32::from(a) + 127) / 255) as u8);
            [r, g, b, a]
        })
        .collect();
    if !WORDMARK_ALPHA.contains(&255) {
        return Err("wordmark bitmap has no opaque pixel".into());
    }
    let failed = dimmed(&rgba);
    Ok(Wordmark { rgba, failed })
}

/// Premultiplies an RGBA32 surface in place (`SDL_PremultiplySurfaceAlpha`) and returns its
/// tightly packed bytes.
fn premultiplied_rgba(surface: Surface<'_>) -> Result<Vec<u8>, String> {
    // SAFETY: a valid, unlocked RGBA32 surface owned here.
    if !unsafe { sdl3::sys::surface::SDL_PremultiplySurfaceAlpha(surface.raw(), false) } {
        return Err(format!("premultiply: {}", sdl3::get_error()));
    }
    Ok(packed_rgba(&surface))
}

/// Premultiplied RGBA8 at `FAILED_LOGO_ALPHA` opacity.
fn dimmed(rgba: &[u8]) -> Vec<u8> {
    rgba.iter()
        .map(|&c| ((u32::from(c) * FAILED_LOGO_ALPHA + 127) / 255) as u8)
        .collect()
}

/// `SDL_ScaleSurface` with linear filtering to an `edge` square.
fn scale(surface: &Surface<'_>, edge: u32) -> Result<Surface<'static>, String> {
    let edge_i32 = i32::try_from(edge).map_err(|e| format!("scale: {e}"))?;
    // SAFETY: `surface` is valid; SDL returns a new surface owned by the caller or null.
    let raw = unsafe {
        sdl3::sys::surface::SDL_ScaleSurface(
            surface.raw(),
            edge_i32,
            edge_i32,
            sdl3::sys::surface::SDL_SCALEMODE_LINEAR,
        )
    };
    if raw.is_null() {
        return Err(format!("scale to {edge}: {}", sdl3::get_error()));
    }
    // SAFETY: non-null and owned; `Surface` frees it on drop.
    let scaled = unsafe { Surface::from_ll(raw) };
    scaled
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| format!("scale format: {e}"))
}

/// Copies the rows of an RGBA32 surface without the pitch padding.
fn packed_rgba(surface: &Surface<'_>) -> Vec<u8> {
    let (width, height, pitch) = (
        surface.width() as usize,
        surface.height() as usize,
        surface.pitch() as usize,
    );
    surface.with_lock(|bytes| {
        (0..height)
            .flat_map(|row| &bytes[row * pitch..row * pitch + width * 4])
            .copied()
            .collect()
    })
}
