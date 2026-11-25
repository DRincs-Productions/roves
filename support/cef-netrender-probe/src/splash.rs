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
//! keep the Roves logo either way. The icon decoding itself is platform-neutral and shared
//! with the macOS and Linux players (`window_icon.rs`).

use sdl3::{pixels::PixelFormat, surface::Surface};

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
    let surface = crate::window_icon::roves_logo()?;
    if (surface.width(), surface.height()) != (LOGO_PX, LOGO_PX) {
        return Err(format!(
            "Roves logo is {}x{}, expected {LOGO_PX}x{LOGO_PX}",
            surface.width(),
            surface.height()
        ));
    }
    let icon = surface
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| format!("icon copy: {e}"))?;
    let logo = premultiplied_rgba(surface)?;
    let failed = dimmed(&logo);
    Ok(Branding { icon, logo, failed })
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
