//! Startup screen of the macOS and Linux players (the Linux gate through `#[path]`), shown
//! in packaged-game mode from the first present until the first game frame after the
//! accepted load-end (README "Startup screen"). Same content and rules as the Windows player:
//! the dark background, the Roves logo and the MetalMania "Roves" wordmark of its `splash.rs`
//! (logo decoded by SDL3, wordmark rasterized by the shared `build_wordmark.rs`) and the
//! indeterminate loading bar, placed by its `game_rules::splash_layout` /
//! `splash_indicator`. The Windows player draws them with its own wgpu blit; here the static
//! column is composed on the CPU once per window size into one texture that wgpu's
//! `TextureBlitter` (the frame path's blit) shows 1:1, and only the bar's few rows are
//! rewritten each frame. No failure screen (the gates end the run instead).

use std::time::Instant;

use crate::{
    game_rules::{splash_indicator, splash_layout},
    splash::{
        BAR_INDICATOR, BAR_SIZE, BAR_TRACK, LOGO_PX, SPLASH_BACKGROUND, SPLASH_GAP, WORDMARK_SIZE,
    },
};

pub(crate) struct StartupScreen {
    /// `LOGO_PX` square, premultiplied RGBA8; `None` when decoding failed (omitted).
    logo: Option<Vec<u8>>,
    /// `WORDMARK_SIZE`, premultiplied RGBA8; `None` when unavailable (omitted).
    wordmark: Option<Vec<u8>>,
    /// The canvas format: sRGB exactly when the surface is, so the stored bytes reach the
    /// screen unchanged through the blit (as the CEF frames, which are sRGB-encoded bytes).
    format: wgpu::TextureFormat,
    canvas: Option<Canvas>,
    /// Origin of the loading-bar animation.
    started: Instant,
}

struct Canvas {
    view: wgpu::TextureView,
    texture: wgpu::Texture,
    /// Top-left corner of the loading bar, `None` when the window is too small for it.
    bar_at: Option<(u32, u32)>,
}

impl StartupScreen {
    /// Decodes the branding (one log line each; a failure only omits that piece).
    pub(crate) fn new(surface_format: wgpu::TextureFormat) -> Self {
        let logo = match crate::splash::decode() {
            Ok(branding) => {
                println!("gate: startup logo decoded size={LOGO_PX}x{LOGO_PX}");
                Some(branding.logo)
            },
            Err(error) => {
                println!("gate: startup logo unavailable error={error}");
                None
            },
        };
        let wordmark = match crate::splash::wordmark() {
            Ok(wordmark) => {
                let (w, h) = WORDMARK_SIZE;
                println!("gate: wordmark rendered size={w}x{h} font=MetalMania");
                Some(wordmark.rgba)
            },
            Err(error) => {
                println!("gate: wordmark unavailable error={error}");
                None
            },
        };
        let format = if surface_format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        StartupScreen {
            logo,
            wordmark,
            format,
            canvas: None,
            started: Instant::now(),
        }
    }

    /// Composes the static column (background, logo, wordmark) for a `width` x `height`
    /// target; called once and on every surface resize.
    pub(crate) fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        let background = SPLASH_BACKGROUND.map(|c| (c * 255.0).round() as u8);
        let mut pixels = background.repeat((w * h) as usize);
        let logo = self.logo.as_deref().map(|rgba| (rgba, (LOGO_PX, LOGO_PX)));
        let wordmark = self.wordmark.as_deref().map(|rgba| (rgba, WORDMARK_SIZE));
        let [logo_at, wordmark_at, bar_at] = splash_layout(
            w,
            h,
            [
                logo.map(|(_, size)| size),
                wordmark.map(|(_, size)| size),
                Some(BAR_SIZE),
            ],
            SPLASH_GAP,
        );
        for (item, at) in [(logo, logo_at), (wordmark, wordmark_at)] {
            if let (Some((rgba, size)), Some(at)) = (item, at) {
                over(&mut pixels, w, rgba, size, at);
            }
        }
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("roves startup screen"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write(queue, &texture, (0, 0), (w, h), &pixels);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.canvas = Some(Canvas {
            view,
            texture,
            bar_at,
        });
    }

    /// Updates the loading bar and blits the screen onto `target` (same size as the last
    /// `resize`).
    pub(crate) fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        blitter: &wgpu::util::TextureBlitter,
        target: &wgpu::TextureView,
    ) {
        let Some(canvas) = self.canvas.as_ref() else {
            return;
        };
        if let Some(at) = canvas.bar_at {
            let (bw, bh) = BAR_SIZE;
            let mut row: Vec<u8> = BAR_TRACK.repeat(bw as usize);
            let elapsed = self.started.elapsed().as_millis();
            if let Some((offset, width)) = splash_indicator(bw, elapsed) {
                let span = (offset * 4) as usize..((offset + width) * 4) as usize;
                for texel in row[span].chunks_exact_mut(4) {
                    texel.copy_from_slice(&BAR_INDICATOR);
                }
            }
            write(
                queue,
                &canvas.texture,
                at,
                BAR_SIZE,
                &row.repeat(bh as usize),
            );
        }
        blitter.copy(device, encoder, &canvas.view, target);
    }
}

/// Premultiplied source-over of a tightly packed `size` RGBA8 image at `at` into the
/// `width`-wide opaque canvas.
fn over(canvas: &mut [u8], width: u32, rgba: &[u8], (w, h): (u32, u32), (x, y): (u32, u32)) {
    for row in 0..h {
        let src = &rgba[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
        let start = (((y + row) * width + x) * 4) as usize;
        let dst = &mut canvas[start..start + (w * 4) as usize];
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            let keep = 255 - u32::from(s[3]);
            for (dc, &sc) in d.iter_mut().zip(s) {
                *dc = (u32::from(sc) + (u32::from(*dc) * keep + 127) / 255).min(255) as u8;
            }
        }
    }
}

/// Writes a tightly packed RGBA8 `size` region at `at`.
fn write(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    (x, y): (u32, u32),
    (w, h): (u32, u32),
    rgba: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
}
