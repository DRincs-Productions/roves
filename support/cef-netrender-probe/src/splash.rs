//! Game-mode branding: the official Roves logo (repository root `icon.png`, embedded at
//! build time) decoded and downscaled by SDL3 itself (`SDL_LoadPNG_IO`, SDL's bundled
//! stb_image, and `SDL_ScaleSurface`), no extra image crate. It becomes the window icon
//! (`SDL_SetWindowIcon`) and the logo texture of the dark startup/failure screen, which the
//! existing wgpu blit composes and presents (README "Packaged player").

use sdl3::{iostream::IOStream, pixels::PixelFormat, surface::Surface};

const ICON_PNG: &[u8] = include_bytes!("../../../icon.png");
/// Logo edge in physical pixels: the 1024 px source halved twice (each halving with linear
/// filtering averages exactly 2x2 texels).
pub(crate) const LOGO_PX: u32 = 256;
/// Dark startup-screen background (#141418), also the clear colour around the game frame
/// in game mode. The composed target is `Rgba8Unorm`, so these are the stored bytes / 255.
pub(crate) const SPLASH_BACKGROUND: [f32; 4] = [20.0 / 255.0, 20.0 / 255.0, 24.0 / 255.0, 1.0];
/// Opacity of the logo on the failure screen (out of 255).
const FAILED_LOGO_ALPHA: u32 = 80;

pub(crate) struct Branding {
    /// `LOGO_PX` square, straight alpha: `SDL_SetWindowIcon` input.
    pub(crate) icon: Surface<'static>,
    /// Tightly packed premultiplied RGBA8 of `LOGO_PX` x `LOGO_PX` (startup screen).
    pub(crate) logo: Vec<u8>,
    /// The same logo at `FAILED_LOGO_ALPHA` opacity (failure screen).
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
    while surface.width() > LOGO_PX {
        let next = (surface.width() / 2).max(LOGO_PX);
        surface = scale(&surface, next)?;
    }
    let icon = surface
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| format!("icon copy: {e}"))?;
    let premultiplied = surface;
    // SAFETY: a valid, unlocked RGBA32 surface owned here.
    if !unsafe { sdl3::sys::surface::SDL_PremultiplySurfaceAlpha(premultiplied.raw(), false) } {
        return Err(format!("premultiply: {}", sdl3::get_error()));
    }
    let logo = packed_rgba(&premultiplied);
    let failed = logo
        .iter()
        .map(|&c| ((u32::from(c) * FAILED_LOGO_ALPHA + 127) / 255) as u8)
        .collect();
    Ok(Branding { icon, logo, failed })
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
