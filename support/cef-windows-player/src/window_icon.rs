//! Window icon of the three CEF players (Windows `splash.rs`, and the macOS and Linux players
//! through `#[path]`): the official Roves logo (repository root `icon.png`, embedded at build
//! time) and the project icon contract (README "Project icon"), both decoded and downscaled
//! by SDL3 itself (`SDL_LoadPNG_IO`, SDL's bundled stb_image, and `SDL_ScaleSurface`), no
//! extra image crate. A valid project `icon.png` in the player's icon directory (next to
//! `play.exe` on Windows and to the executable on Linux, `Contents/Resources` in the macOS
//! `.app`) replaces the Roves logo; the header limits are `icon_rules.rs`.

use std::path::Path;

use sdl3::{iostream::IOStream, pixels::PixelFormat, surface::Surface};

const ICON_PNG: &[u8] = include_bytes!("../../../icon.png");

/// The Roves logo downscaled to `icon_rules::WINDOW_ICON_PX`, RGBA32 with straight alpha.
pub(crate) fn roves_logo() -> Result<Surface<'static>, String> {
    use crate::icon_rules::{WINDOW_ICON_PX, downscale_steps};
    let mut stream = IOStream::from_bytes(ICON_PNG).map_err(|e| format!("icon stream: {e}"))?;
    let mut surface = Surface::load_png_io(&mut stream)
        .and_then(|s| s.convert_format(PixelFormat::RGBA32))
        .map_err(|e| format!("icon.png decode: {e}"))?;
    if surface.width() != surface.height() || surface.width() < WINDOW_ICON_PX {
        return Err(format!(
            "icon.png is {}x{}, expected a square of at least {WINDOW_ICON_PX} px",
            surface.width(),
            surface.height()
        ));
    }
    for edge in downscale_steps(surface.width()) {
        surface = scale(&surface, edge)?;
    }
    Ok(surface)
}

/// Where the window icon comes from.
pub(crate) enum ProjectIcon {
    /// A valid project `icon.png`, downscaled to at most `WINDOW_ICON_PX`, straight alpha.
    Found {
        icon: Surface<'static>,
        source_px: u32,
    },
    /// No `icon.png` in the icon directory: the Roves logo stays.
    Absent,
    /// An `icon.png` that breaks the contract: the Roves logo stays, the reason is logged.
    Invalid(String),
}

/// The project icon in `dir` (`icon_rules::PROJECT_ICON_FILE`), decoded by SDL3 like the
/// Roves logo after the header checks of `icon_rules` (size, PNG signature, square bounds).
pub(crate) fn project_icon(dir: Option<&Path>) -> ProjectIcon {
    use crate::icon_rules::{MAX_ICON_BYTES, PROJECT_ICON_FILE, check_png, downscale_steps};
    let Some(path) = dir.map(|dir| dir.join(PROJECT_ICON_FILE)) else {
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

/// Sets the window icon: the project icon of `dir` when valid, the Roves logo otherwise.
/// Returns the log line (`window icon source=...`). macOS and Linux players; the Windows
/// player logs the two steps separately (`app.rs`).
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn apply(window: &mut sdl3::video::Window, dir: Option<&Path>) -> String {
    let reason = match project_icon(dir) {
        ProjectIcon::Found { icon, source_px } => {
            let set = window.set_icon(&icon);
            if set {
                return format!("window icon source=project size={source_px}x{source_px} set=true");
            }
            "project icon not set".to_owned()
        },
        ProjectIcon::Absent => "absent".to_owned(),
        ProjectIcon::Invalid(error) => format!("invalid error={error}"),
    };
    match roves_logo() {
        Ok(logo) => format!(
            "window icon source=roves reason={reason} set={}",
            window.set_icon(&logo)
        ),
        Err(error) => format!("window icon source=none reason={reason} error={error}"),
    }
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
