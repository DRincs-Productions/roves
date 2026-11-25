// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! CPU-bitmap fallback: CEF's standard software off-screen path.
//!
//! With `CefWindowInfo::shared_texture_enabled = 0` a windowless browser
//! delivers `CefRenderHandler::OnPaint(type, dirtyRects, buffer, width,
//! height)` instead of `OnAcceleratedPaint`. The buffer is `width * height * 4`
//! bytes of BGRA pixels (upper-left origin), owned by CEF and valid only for
//! the callback; `dirtyRects` are in pixels of that buffer. CEF still paints
//! at `CefBrowserSettings::windowless_frame_rate` and only when something
//! changed.
//!
//! [`CpuPaintSurface`] keeps a shadow copy of the newest buffer (only the dirty
//! rows are copied inside the callback) plus the union of the rectangles not
//! yet uploaded. The host thread later uploads that union with
//! `Queue::write_texture` into one persistent texture that is recreated only
//! when the size changes, and hands it out as an ordinary [`ImportedTexture`],
//! so a host presents it exactly like an imported accelerated frame.

use std::sync::{Arc, Mutex};

use dpi::PhysicalSize;

#[cfg(feature = "cpu-paint-fallback")]
use crate::native_frame::{CpuBitmap, NativeFrame};
use crate::{
    native_frame::{HostWgpuContext, ImportedTexture},
    surface::{CefSurfaceConfig, CefSurfaceMode},
};

/// Software-mode state of one producer: the view and the popup element.
///
/// A producer holds `Some` exactly when it created its browser with
/// `shared_texture_enabled = 0`, so CEF calls `OnPaint` and never
/// `OnAcceleratedPaint`.
#[derive(Clone)]
pub(crate) struct SoftwarePaint {
    view: Arc<CpuPaintSurface>,
    popup: Arc<CpuPaintSurface>,
}

impl SoftwarePaint {
    /// `Some` when the host asked for software output
    /// (`prefer_accelerated == false`) and the `cpu-paint-fallback` feature
    /// compiled it in; otherwise the producer stays accelerated, as before.
    pub(crate) fn for_config(
        config: &CefSurfaceConfig,
        format: wgpu::TextureFormat,
    ) -> Option<Self> {
        (cfg!(feature = "cpu-paint-fallback") && !config.prefer_accelerated).then(|| {
            SoftwarePaint {
                view: Arc::new(CpuPaintSurface::new(format)),
                popup: Arc::new(CpuPaintSurface::new(format)),
            }
        })
    }

    pub(crate) fn surface_mode(&self) -> CefSurfaceMode {
        #[cfg(feature = "cpu-paint-fallback")]
        {
            CefSurfaceMode::CpuPaint
        }
        #[cfg(not(feature = "cpu-paint-fallback"))]
        {
            CefSurfaceMode::Unsupported
        }
    }

    /// Route one `OnPaint` callback to the view or the popup element.
    ///
    /// # Safety
    /// As [`CpuPaintSurface::paint`]: `buffer` is CEF's callback-scoped
    /// `width * height * 4` byte BGRA buffer.
    #[cfg(feature = "cef-runtime")]
    pub(crate) unsafe fn on_paint(
        &self,
        type_: cef::PaintElementType,
        dirty_rects: Option<&[cef::Rect]>,
        buffer: *const u8,
        width: i32,
        height: i32,
    ) {
        let target = if type_ == cef::PaintElementType::POPUP {
            &self.popup
        } else {
            &self.view
        };
        let dirty: Vec<_> = dirty_rects
            .unwrap_or(&[])
            .iter()
            .map(|r| (r.x, r.y, r.width, r.height))
            .collect();
        // SAFETY: forwarded from the caller.
        unsafe { target.paint(buffer, width, height, &dirty) };
    }

    pub(crate) fn acquire_view(&self, ctx: &HostWgpuContext) -> Option<ImportedTexture> {
        self.view.acquire(ctx)
    }

    pub(crate) fn acquire_popup(&self, ctx: &HostWgpuContext) -> Option<ImportedTexture> {
        self.popup.acquire(ctx)
    }

    /// A hidden popup never paints again; drop what it painted.
    pub(crate) fn clear_popup(&self) {
        self.popup.clear();
    }

    /// The newest view buffer as an owned native frame (escape hatch).
    #[cfg(feature = "cpu-paint-fallback")]
    pub(crate) fn take_native(&self) -> Option<NativeFrame> {
        self.view.take_native()
    }

    #[cfg(not(feature = "cpu-paint-fallback"))]
    pub(crate) fn take_native(&self) -> Option<crate::native_frame::NativeFrame> {
        None
    }
}

/// A rectangle in buffer pixels, already clamped to the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn full(size: PhysicalSize<u32>) -> Self {
        PixelRect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }

    /// Clamp a CEF rectangle (`x, y, width, height`, possibly negative or
    /// overhanging) to `size`; `None` when nothing of it is inside.
    fn clamped(
        (x, y, width, height): (i32, i32, i32, i32),
        size: PhysicalSize<u32>,
    ) -> Option<Self> {
        let x0 = i64::from(x).clamp(0, i64::from(size.width));
        let y0 = i64::from(y).clamp(0, i64::from(size.height));
        let x1 = (i64::from(x) + i64::from(width)).clamp(0, i64::from(size.width));
        let y1 = (i64::from(y) + i64::from(height)).clamp(0, i64::from(size.height));
        (x1 > x0 && y1 > y0).then(|| PixelRect {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        })
    }

    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        PixelRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}

#[derive(Default)]
struct Shadow {
    /// Newest full BGRA buffer, tightly packed (`size.width * 4` per row).
    pixels: Vec<u8>,
    size: PhysicalSize<u32>,
    /// Union of the regions painted since the last upload (or native take).
    pending: Option<PixelRect>,
    generation: u64,
    /// Persistent upload target, recreated when the size changes.
    target: Option<(wgpu::Texture, wgpu::TextureView)>,
}

/// Latest-frame state of one paint element (view or popup) in software mode.
pub(crate) struct CpuPaintSurface {
    format: wgpu::TextureFormat,
    shadow: Mutex<Shadow>,
}

impl CpuPaintSurface {
    /// `format` is the texture format the platform's accelerated frames use
    /// for the same BGRA bytes, so hosts see one format in either mode.
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        CpuPaintSurface {
            format,
            shadow: Mutex::new(Shadow::default()),
        }
    }

    /// Record one `OnPaint` buffer. Called inside the callback: only the dirty
    /// rows are copied, or the whole buffer when the size changed.
    ///
    /// # Safety
    /// `buffer` must be null or point to `width * height * 4` readable bytes
    /// for the duration of the call, as CEF guarantees for `OnPaint`.
    pub(crate) unsafe fn paint(
        &self,
        buffer: *const u8,
        width: i32,
        height: i32,
        dirty: &[(i32, i32, i32, i32)],
    ) {
        if buffer.is_null() || width <= 0 || height <= 0 {
            return;
        }
        let size = PhysicalSize::new(width as u32, height as u32);
        let len = size.width as usize * size.height as usize * 4;
        // SAFETY: the caller guarantees `len` readable bytes behind `buffer`.
        let source = unsafe { std::slice::from_raw_parts(buffer, len) };
        let mut shadow = self.shadow.lock().unwrap();
        let resized = shadow.size != size || shadow.pixels.len() != len;
        let painted = if resized || dirty.is_empty() {
            shadow.pixels.clear();
            shadow.pixels.extend_from_slice(source);
            if resized {
                shadow.size = size;
                shadow.target = None;
            }
            Some(PixelRect::full(size))
        } else {
            let stride = size.width as usize * 4;
            let mut painted: Option<PixelRect> = None;
            for rect in dirty.iter().filter_map(|&r| PixelRect::clamped(r, size)) {
                for row in rect.y..rect.y + rect.height {
                    let start = row as usize * stride + rect.x as usize * 4;
                    let end = start + rect.width as usize * 4;
                    shadow.pixels[start..end].copy_from_slice(&source[start..end]);
                }
                painted = Some(painted.map_or(rect, |p| p.union(rect)));
            }
            painted
        };
        let Some(painted) = painted else { return };
        shadow.pending = Some(shadow.pending.map_or(painted, |p| p.union(painted)));
        shadow.generation += 1;
    }

    /// Forget the current content (a hidden popup never paints again).
    pub(crate) fn clear(&self) {
        let mut shadow = self.shadow.lock().unwrap();
        shadow.pending = None;
    }

    /// The newest buffer as an owned [`NativeFrame::CpuBitmap`], if it changed
    /// since the last take or upload. Copies the whole buffer.
    #[cfg(feature = "cpu-paint-fallback")]
    pub(crate) fn take_native(&self) -> Option<NativeFrame> {
        let mut shadow = self.shadow.lock().unwrap();
        shadow.pending.take()?;
        Some(NativeFrame::CpuBitmap(CpuBitmap {
            data: shadow.pixels.clone(),
            size: shadow.size,
            format: self.format,
            generation: shadow.generation,
        }))
    }

    /// Upload what changed since the last call into the persistent texture and
    /// return it; `None` when nothing was painted meanwhile.
    pub(crate) fn acquire(&self, ctx: &HostWgpuContext) -> Option<ImportedTexture> {
        let mut shadow = self.shadow.lock().unwrap();
        let mut region = shadow.pending.take()?;
        let size = shadow.size;
        let extent = wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        };
        if shadow.target.is_none() {
            let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("cef_cpu_paint"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            shadow.target = Some((texture, view));
            region = PixelRect::full(size);
        }
        let (texture, view) = shadow.target.clone()?;
        let offset = (region.y as usize * size.width as usize + region.x as usize) * 4;
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &shadow.pixels[offset..],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width * 4),
                rows_per_image: Some(region.height),
            },
            wgpu::Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
        Some(ImportedTexture {
            texture,
            view,
            size: extent,
            format: self.format,
            generation: shadow.generation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(width: u32, height: u32, fill: u8) -> Vec<u8> {
        vec![fill; (width * height * 4) as usize]
    }

    #[test]
    fn rect_clamps_to_the_buffer() {
        let size = PhysicalSize::new(10, 8);
        assert_eq!(
            PixelRect::clamped((-2, -3, 5, 5), size),
            Some(PixelRect {
                x: 0,
                y: 0,
                width: 3,
                height: 2
            })
        );
        assert_eq!(
            PixelRect::clamped((8, 6, 5, 5), size),
            Some(PixelRect {
                x: 8,
                y: 6,
                width: 2,
                height: 2
            })
        );
        assert_eq!(PixelRect::clamped((10, 0, 3, 3), size), None);
        assert_eq!(PixelRect::clamped((0, 0, 0, 3), size), None);
    }

    #[test]
    fn first_paint_is_full_and_dirty_paints_copy_only_their_rows() {
        let surface = CpuPaintSurface::new(wgpu::TextureFormat::Bgra8Unorm);
        let first = bitmap(4, 3, 1);
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(first.as_ptr(), 4, 3, &[(1, 1, 1, 1)]) };
        {
            let shadow = surface.shadow.lock().unwrap();
            assert_eq!(
                shadow.pending,
                Some(PixelRect::full(PhysicalSize::new(4, 3)))
            );
            assert!(shadow.pixels.iter().all(|&b| b == 1));
        }
        surface.shadow.lock().unwrap().pending = None;

        // Same size: only the dirty texel (1,1) is copied from the new buffer.
        let second = bitmap(4, 3, 9);
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(second.as_ptr(), 4, 3, &[(1, 1, 1, 1)]) };
        let shadow = surface.shadow.lock().unwrap();
        assert_eq!(
            shadow.pending,
            Some(PixelRect {
                x: 1,
                y: 1,
                width: 1,
                height: 1
            })
        );
        let texel = (4 + 1) * 4;
        assert_eq!(&shadow.pixels[texel..texel + 4], &[9, 9, 9, 9]);
        assert_eq!(shadow.pixels.iter().filter(|&&b| b == 9).count(), 4);
        assert_eq!(shadow.generation, 2);
    }

    #[cfg(feature = "cpu-paint-fallback")]
    #[test]
    fn pending_regions_accumulate_until_taken() {
        let surface = CpuPaintSurface::new(wgpu::TextureFormat::Bgra8Unorm);
        let pixels = bitmap(8, 8, 0);
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(pixels.as_ptr(), 8, 8, &[]) };
        surface.shadow.lock().unwrap().pending = None;
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(pixels.as_ptr(), 8, 8, &[(0, 0, 2, 2)]) };
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(pixels.as_ptr(), 8, 8, &[(5, 6, 1, 1)]) };
        assert_eq!(
            surface.shadow.lock().unwrap().pending,
            Some(PixelRect {
                x: 0,
                y: 0,
                width: 6,
                height: 7
            })
        );
        let frame = surface.take_native().expect("a frame was painted");
        assert_eq!(frame.size(), PhysicalSize::new(8, 8));
        assert_eq!(frame.generation(), 3);
        assert!(surface.take_native().is_none());
    }

    #[test]
    fn resize_resets_the_target_and_null_buffers_are_ignored() {
        let surface = CpuPaintSurface::new(wgpu::TextureFormat::Bgra8Unorm);
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(std::ptr::null(), 4, 4, &[]) };
        assert!(surface.shadow.lock().unwrap().pending.is_none());
        let small = bitmap(2, 2, 3);
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(small.as_ptr(), 2, 2, &[]) };
        let large = bitmap(3, 2, 4);
        // SAFETY: the buffer holds width * height * 4 bytes (or is null).
        unsafe { surface.paint(large.as_ptr(), 3, 2, &[(0, 0, 1, 1)]) };
        let shadow = surface.shadow.lock().unwrap();
        assert_eq!(shadow.size, PhysicalSize::new(3, 2));
        assert_eq!(
            shadow.pending,
            Some(PixelRect::full(PhysicalSize::new(3, 2)))
        );
        assert!(shadow.pixels.iter().all(|&b| b == 4));
    }
}
