//! Pure rules of the project window icon (std only, unit-tested stand-alone:
//! `rustc --edition 2024 --test src/icon_rules.rs`).
//!
//! Contract (README "Project icon"): packaging tools copy the game's icon as `icon.png` next
//! to `play.exe`; the player uses it as the SDL window/title-bar icon and falls back to the
//! embedded Roves logo when it is absent or does not satisfy these rules. The startup and
//! failure screens always show the Roves logo. The same limits are enforced by the packaging
//! tools (engine workflow, roves-action `cef-windows`, Roves Packmaster) before copying, so a
//! file they accept is never rejected here.

/// File name of the project icon next to `play.exe`.
pub(crate) const PROJECT_ICON_FILE: &str = "icon.png";
/// Largest accepted file, checked before reading it.
pub(crate) const MAX_ICON_BYTES: u64 = 8 * 1024 * 1024;
/// Accepted edge of the (square) icon, in pixels, checked on the PNG header before decoding.
pub(crate) const MIN_ICON_PX: u32 = 16;
pub(crate) const MAX_ICON_PX: u32 = 2048;
/// Edge the window icon is downscaled to (larger sources only).
pub(crate) const WINDOW_ICON_PX: u32 = 256;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Width and height from the PNG signature and the `IHDR` chunk, which must come first.
pub(crate) fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 33 || &bytes[..8] != PNG_SIGNATURE {
        return Err("not a PNG file (signature or IHDR missing)".into());
    }
    let length = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if length != 13 || &bytes[12..16] != b"IHDR" {
        return Err("not a PNG file (first chunk is not a 13-byte IHDR)".into());
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Ok((width, height))
}

/// Square, between `MIN_ICON_PX` and `MAX_ICON_PX`.
pub(crate) fn check_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width != height {
        return Err(format!("{width}x{height} is not square"));
    }
    if !(MIN_ICON_PX..=MAX_ICON_PX).contains(&width) {
        return Err(format!(
            "{width}x{height} is outside {MIN_ICON_PX}..={MAX_ICON_PX} px"
        ));
    }
    Ok(())
}

/// Header-level checks of a candidate `icon.png`: size, PNG signature, square bounds.
/// Decoding (SDL3) can still reject a damaged body.
pub(crate) fn check_png(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return Err(format!(
            "{} bytes exceeds the {MAX_ICON_BYTES}-byte limit",
            bytes.len()
        ));
    }
    let (width, height) = png_dimensions(bytes)?;
    check_dimensions(width, height)?;
    Ok((width, height))
}

/// Successive edges a square `edge` icon is scaled through (each step at most halves, so
/// linear filtering stays a box filter) until it is at most `WINDOW_ICON_PX`.
pub(crate) fn downscale_steps(mut edge: u32) -> Vec<u32> {
    let mut steps = Vec::new();
    while edge > WINDOW_ICON_PX {
        edge = (edge / 2).max(WINDOW_ICON_PX);
        steps.push(edge);
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    #[test]
    fn accepts_square_icons_within_bounds() {
        assert_eq!(check_png(&header(512, 512)), Ok((512, 512)));
        assert!(check_png(&header(MIN_ICON_PX, MIN_ICON_PX)).is_ok());
        assert!(check_png(&header(MAX_ICON_PX, MAX_ICON_PX)).is_ok());
    }

    #[test]
    fn rejects_malformed_or_out_of_bounds_icons() {
        assert!(check_png(b"").is_err());
        assert!(check_png(b"GIF89a not a png at all, padded to be long enough").is_err());
        let mut wrong_chunk = header(64, 64);
        wrong_chunk[12..16].copy_from_slice(b"IDAT");
        assert!(check_png(&wrong_chunk).is_err());
        assert!(
            check_png(&header(64, 32))
                .unwrap_err()
                .contains("not square")
        );
        assert!(check_png(&header(8, 8)).is_err());
        assert!(check_png(&header(4096, 4096)).is_err());
        assert!(check_png(&header(0, 0)).is_err());
        let mut huge = header(64, 64);
        huge.resize(MAX_ICON_BYTES as usize + 1, 0);
        assert!(check_png(&huge).unwrap_err().contains("limit"));
    }

    #[test]
    fn downscales_by_at_most_halving() {
        assert!(downscale_steps(256).is_empty());
        assert!(downscale_steps(48).is_empty());
        assert_eq!(downscale_steps(1024), vec![512, 256]);
        assert_eq!(downscale_steps(2048), vec![1024, 512, 256]);
        assert_eq!(downscale_steps(500), vec![256]);
        assert_eq!(downscale_steps(1500), vec![750, 375, 256]);
    }
}
