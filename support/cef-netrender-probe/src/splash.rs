//! Game-mode branding: the official Roves logo (repository root `icon.png`, embedded at
//! build time) decoded and downscaled by SDL3 itself (`SDL_LoadPNG_IO`, SDL's bundled
//! stb_image, and `SDL_ScaleSurface`), no extra image crate. It becomes the window icon
//! (`SDL_SetWindowIcon`) and the logo texture of the dark startup/failure screen. The
//! "Roves" wordmark under it is drawn by SDL3 too (`SDL_RenderDebugText`, SDL's built-in 8x8
//! font, on a software renderer), and the indeterminate loading bar is two flat colours. The
//! existing wgpu blit composes and presents all of them (README "Packaged player").

use sdl3::{
    iostream::IOStream,
    pixels::{Color, PixelFormat},
    surface::Surface,
};

const ICON_PNG: &[u8] = include_bytes!("../../../icon.png");
/// Logo edge in physical pixels: the 1024 px source halved twice (each halving with linear
/// filtering averages exactly 2x2 texels).
pub(crate) const LOGO_PX: u32 = 256;
/// Dark startup-screen background (#141418), also the clear colour around the game frame
/// in game mode. The composed target is `Rgba8Unorm`, so these are the stored bytes / 255.
pub(crate) const SPLASH_BACKGROUND: [f32; 4] = [20.0 / 255.0, 20.0 / 255.0, 24.0 / 255.0, 1.0];
/// Opacity of the logo and wordmark on the failure screen (out of 255).
const FAILED_LOGO_ALPHA: u32 = 80;

/// The wordmark text and its integer scale of SDL's 8x8 debug font (nearest, so the glyphs
/// stay sharp; SDL marks its debug-font atlas `SDL_SCALEMODE_PIXELART`).
const WORDMARK: &str = "Roves";
const WORDMARK_SCALE: u32 = 6;
const GLYPH_PX: u32 = sdl3::sys::render::SDL_DEBUG_TEXT_FONT_CHARACTER_SIZE as u32;
/// Wordmark texture size in physical pixels (240 x 48).
pub(crate) const WORDMARK_SIZE: (u32, u32) = (
    WORDMARK.len() as u32 * GLYPH_PX * WORDMARK_SCALE,
    GLYPH_PX * WORDMARK_SCALE,
);
/// Wordmark colour, the near-white of the logo (#F2F2F5).
const WORDMARK_COLOR: Color = Color::RGBA(242, 242, 245, 255);
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
    while surface.width() > LOGO_PX {
        let next = (surface.width() / 2).max(LOGO_PX);
        surface = scale(&surface, next)?;
    }
    let icon = surface
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| format!("icon copy: {e}"))?;
    let logo = premultiplied_rgba(surface)?;
    let failed = dimmed(&logo);
    Ok(Branding { icon, logo, failed })
}

/// Draws `WORDMARK` with SDL3's built-in debug font on a software renderer over a
/// transparent RGBA32 surface, scaled by `WORDMARK_SCALE`.
pub(crate) fn wordmark() -> Result<Wordmark, String> {
    let (width, height) = WORDMARK_SIZE;
    let surface = Surface::new(width, height, PixelFormat::RGBA32)
        .map_err(|e| format!("wordmark surface: {e}"))?;
    let mut canvas = surface
        .into_canvas()
        .map_err(|e| format!("wordmark software renderer: {e}"))?;
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 0));
    canvas.clear();
    canvas
        .set_scale(WORDMARK_SCALE as f32, WORDMARK_SCALE as f32)
        .map_err(|e| format!("wordmark scale: {e}"))?;
    canvas.set_draw_color(WORDMARK_COLOR);
    canvas
        .draw_debug_text(WORDMARK, (0.0, 0.0))
        .map_err(|e| format!("wordmark text: {e}"))?;
    // SAFETY: the renderer is alive (owned by `canvas`); flushing executes the queued draw
    // into the surface. A software renderer without a window cannot `present`.
    if !unsafe { sdl3::sys::render::SDL_FlushRenderer(canvas.raw()) } {
        return Err(format!("wordmark flush: {}", sdl3::get_error()));
    }
    let rgba = premultiplied_rgba(canvas.into_surface())?;
    if !rgba.as_chunks::<4>().0.iter().any(|texel| texel[3] == 255) {
        return Err("wordmark rendered no opaque pixel".into());
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
