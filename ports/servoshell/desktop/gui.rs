/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::fs;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dpi::PhysicalSize;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{
    Button, FontData, FontDefinitions, FontFamily, Id, Key, Label, LayerId, Modifiers, Order,
    PaintCallback, Panel, Vec2, WidgetInfo, WidgetType, pos2,
};
use egui_glow::{CallbackFn, EguiGlow};
use egui_winit::EventResponse;
use euclid::{Length, Point2D, Rect, Scale, Size2D};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use log::info;
use log::warn;
use servo::{
    DeviceIndependentPixel, DevicePixel, OffscreenRenderingContext, RenderingContext, WebView,
};
use url::Url;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use crate::desktop::event_loop::AppEvent;
use crate::desktop::headed_window;
use crate::running_app_state::{RunningAppState, UserInterfaceCommand};
use crate::window::ServoShellWindow;

/// The user interface of a headed servoshell. Currently this is implemented via
/// egui.
pub struct Gui {
    rendering_context: Rc<OffscreenRenderingContext>,
    context: EguiGlow,
    toolbar_height: Length<f32, DeviceIndependentPixel>,

    /// The text to display in the status bar on the bottom of the window.
    status_text: Option<String>,

    /// AccessKit tree updates pending the next egui tick.
    /// This allows us to ensure that graft nodes are sent before the subtrees they graft.
    pending_accesskit_updates: Vec<accesskit::TreeUpdate>,

    /// Loaded once, in [`Gui::new`], for [`Gui::update_splash`]'s boot splash — always
    /// `resources/servo_1024.png` regardless of any game-supplied window/taskbar icon (see
    /// `headed_window.rs`'s icon loading), since the splash is explicitly Roves-branded, not
    /// the game's own branding. The 1024px asset, not the 64px one `build.rs`'s window icon
    /// uses — the splash renders this at a size that can exceed 64px once scaled up to
    /// visually match the wordmark (see `SPLASH_ICON_SCALE`), and upscaling a 64px source
    /// that far looked visibly pixelated on a real build; downscaling a larger source never
    /// has that problem.
    splash_icon_texture: egui::TextureHandle,
}

/// Decodes `resources/servo_1024.png` into an `egui::ColorImage` for the boot
/// splash's icon — see [`Gui::update_splash`]. Deliberately independent of
/// `headed_window.rs`'s own (Linux/Windows-only) `load_icon`/winit `Icon`
/// loading: this must work on macOS too, and egui wants a `ColorImage`, not
/// a winit `Icon`.
fn load_splash_icon_image() -> egui::ColorImage {
    let bytes = include_bytes!("../../../resources/servo_1024.png");
    let image = image::load_from_memory(bytes)
        .expect("Failed to load boot splash icon")
        .to_rgba8();
    let (width, height) = image.dimensions();
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], image.as_raw())
}

fn truncate_with_ellipsis(input: &str, max_length: usize) -> String {
    if input.chars().count() > max_length {
        let truncated: String = input.chars().take(max_length.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        input.to_string()
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn load_cjk_fonts(font_candidates: &[(&str, &str)]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut loaded_font_names = Vec::new();

    for (path_str, font_name) in font_candidates.iter() {
        let font_path = Path::new(path_str);
        if font_path.exists() {
            match fs::read(font_path) {
                Ok(bytes) => {
                    if !fonts.font_data.contains_key(*font_name) {
                        fonts
                            .font_data
                            .insert(font_name.to_string(), Arc::new(FontData::from_owned(bytes)));
                        loaded_font_names.push(font_name.to_string());
                        info!("Loaded font: {}", font_name);
                    }
                },
                Err(error) => {
                    info!("Failed to read font {}: {}", font_name, error);
                },
            }
        }
    }

    if !loaded_font_names.is_empty() {
        let proportional = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
        for font_name in loaded_font_names.iter() {
            proportional.insert(0, font_name.clone());
        }
    }

    fonts
}

#[cfg(target_os = "windows")]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (r"C:\Windows\Fonts\malgun.ttf", "Malgun Gothic"), // Korean
        (r"C:\Windows\Fonts\msyh.ttc", "Microsoft YaHei"), // Chinese + Japanese
    ])
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "Noto Sans CJK",
        ), // Ubuntu/Debian
        (
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "Noto Sans CJK",
        ), // Fedora/Arch
        // FreeBSD splits the Noto CJK fonts into regional subsets
        (
            "/usr/local/share/fonts/noto/NotoSansCJKhk-Regular.otf",
            "Noto Sans CJK HK",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKjp-Regular.otf",
            "Noto Sans CJK JP",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKkr-Regular.otf",
            "Noto Sans CJK KR",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKsc-Regular.otf",
            "Noto Sans CJK SC",
        ),
        (
            "/usr/local/share/fonts/noto/NotoSansCJKtc-Regular.otf",
            "Noto Sans CJK TC",
        ),
        (
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "WenQuanYi Micro Hei",
        ), // common fallback
        (
            "/usr/local/share/fonts/wqy/wqy-microhei.ttc",
            "WenQuanYi Micro Hei",
        ), // FreeBSD
    ])
}

#[cfg(target_os = "macos")]
fn configure_fonts() -> FontDefinitions {
    // TODO: Default proportional fonts: ["Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"]
    // does not support CJK. Add them for Mac.
    FontDefinitions::default()
}

/// Registers the bundled Metal Mania display font (`resources/fonts/`, OFL-licensed — see
/// `resources/fonts/MetalMania-OFL.txt`) under its own named family,
/// `FontFamily::Name("metal_mania")`, used only for the "Roves" wordmark on the boot splash
/// (see [`Gui::update_splash`]) — unlike `configure_fonts` above, this isn't a
/// proportional-font *fallback* (nothing else in the UI should render in a display/
/// blackletter font), so it gets its own family instead of being pushed onto
/// `FontFamily::Proportional`'s fallback chain, and runs on every platform, not just
/// where CJK system fonts get probed.
fn add_wordmark_font(fonts: &mut FontDefinitions) {
    const WORDMARK_FONT_NAME: &str = "Metal Mania";
    fonts.font_data.insert(
        WORDMARK_FONT_NAME.to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../../resources/fonts/MetalMania-Regular.ttf"
        ))),
    );
    fonts.families.insert(
        FontFamily::Name(WORDMARK_FONT_NAME.into()),
        vec![WORDMARK_FONT_NAME.to_owned()],
    );
}

/// Boot splash sizing (`Gui::update_splash`). No separate icon-size constant
/// here: the icon is sized to match the wordmark's own measured height at
/// `SPLASH_WORDMARK_FONT_SIZE` (see `update_splash`), not an independently
/// guessed proportion — the two are meant to read as one lockup, the same
/// height, side by side.
const SPLASH_WORDMARK_FONT_SIZE: f32 = 88.0;
const SPLASH_PROGRESS_BAR_WIDTH: f32 = 260.0;
const SPLASH_PROGRESS_BAR_HEIGHT: f32 = 6.0;

/// How wide the sliding highlight in [`draw_splash_progress_bar`]'s indeterminate
/// animation is, as a fraction of the bar's own width.
const SPLASH_PROGRESS_INDICATOR_WIDTH_RATIO: f32 = 0.35;

/// How long one full sweep (track start to end) of that same animation takes.
const SPLASH_PROGRESS_ANIMATION_PERIOD: f32 = 1.4;

/// `resources/servo_64.png`'s own artwork — a wide, oval badge — doesn't fill its
/// square canvas: confirmed directly (rendering `icon.svg` fresh and measuring its
/// actual non-transparent bounding box) that the visible content is only ~78% of the
/// canvas height, the rest being transparent top/bottom padding. Sizing the *image* to
/// match the wordmark's height (see `update_splash`) therefore visibly undersizes the
/// *badge* by that same margin — this compensates so the badge itself, not its padded
/// canvas, ends up the intended height. Deliberately left as a splash-only correction
/// rather than re-cropping the shared icon assets: those also serve as the Windows
/// `.exe`/taskbar icon (`build.rs`) and (once wired up) the macOS `.app` icon, both
/// contexts where square, centered padding is the normal, correct look.
const SPLASH_ICON_CONTENT_HEIGHT_RATIO: f32 = 0.784;

/// Requested directly, after the content-padding compensation above still read as too
/// small next to the wordmark in a real build: the icon should be a distinctly bigger,
/// more prominent mark, not merely height-matched to the text. Applied on top of
/// `SPLASH_ICON_CONTENT_HEIGHT_RATIO` (see `update_splash`), not instead of it — that
/// compensation is a measured correction for the asset's own padding, this is a separate,
/// deliberate design choice on top of the now-correctly-measured size.
///
/// Was `2.0` — once `fit_to_exact_size` (see `update_splash`) made this scale actually
/// take visible effect for the first time, `2.0` read as too big on a real build; halved
/// to `1.0` (requested directly).
const SPLASH_ICON_SCALE: f32 = 1.0;

/// Draws the boot splash's progress bar track and an indeterminate animated fill
/// directly via `Ui::painter`, rather than `egui::ProgressBar` — that widget draws its
/// track in `visuals.extreme_bg_color`, which under this app's light fallback theme
/// (`Gui::new`'s `options.fallback_theme`) is pure white, identical to the fill color
/// the splash also wants; the two were indistinguishable, so the "bar" just read as a
/// static white rectangle rather than a loading indicator.
///
/// This used to instead be a *determinate* bar, filled to the real fraction of boot
/// extraction completed. Dropped (asked directly, after it read as fake/broken on a real
/// launch): `extract_boot_with_progress` only reports progress per whole boot pack
/// (`support/content-packer/src/extract.rs`), and the boot set is deliberately just the
/// page's own HTML plus whatever it directly references — "usually only one or two
/// packs" per that function's own doc comment — so on the overwhelmingly common case (a
/// cache hit from a previous launch, or any small boot set) the very first progress
/// report already reads `1.0`, before a single visible frame at any other value ever
/// gets painted. The bar was therefore, in practice, always full and never animated,
/// which reads as broken rather than as "already done". Separately, this splash also
/// now stays up through the page-load wait that follows extraction (see
/// `headed_window.rs`'s `page_load_splash_since`), which has no fractional signal *at
/// all* (`LoadStatus` is a 3-state enum, not a percentage). Given neither phase has a
/// completion fraction worth trusting, this instead always draws a dim, translucent
/// track with a brighter outline, and a fixed-width white highlight that sweeps back and
/// forth across it — driven purely by `elapsed` (wall-clock time since the current wait
/// began, see `HeadedWindow::paint_splash`), so it's continuously, honestly "doing
/// something" for as long as the splash is up, instead of asserting a specific,
/// frequently-wrong amount of completion.
fn draw_splash_progress_bar(ui: &mut egui::Ui, elapsed: Duration) {
    let (outer_rect, _response) = ui.allocate_exact_size(
        egui::vec2(SPLASH_PROGRESS_BAR_WIDTH, SPLASH_PROGRESS_BAR_HEIGHT),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(outer_rect) {
        return;
    }
    let corner_radius = egui::CornerRadius::same(2);
    let painter = ui.painter();
    painter.rect_filled(
        outer_rect,
        corner_radius,
        egui::Color32::from_white_alpha(28),
    );
    painter.rect_stroke(
        outer_rect,
        corner_radius,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(90)),
        egui::StrokeKind::Outside,
    );

    let indicator_width = outer_rect.width() * SPLASH_PROGRESS_INDICATOR_WIDTH_RATIO;
    let travel = (outer_rect.width() - indicator_width).max(0.0);
    // Triangle wave in [0, 1]: sweeps start-to-end over the first half of the period,
    // then end-to-start over the second, so the highlight ping-pongs indefinitely.
    let phase = (elapsed.as_secs_f32() / SPLASH_PROGRESS_ANIMATION_PERIOD).rem_euclid(2.0);
    let ping_pong = if phase <= 1.0 { phase } else { 2.0 - phase };
    let indicator_rect = egui::Rect::from_min_size(
        outer_rect.min + egui::vec2(travel * ping_pong, 0.0),
        egui::vec2(indicator_width, outer_rect.height()),
    );
    painter.rect_filled(indicator_rect, corner_radius, egui::Color32::WHITE);
}

impl Drop for Gui {
    fn drop(&mut self) {
        self.rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");
        self.context.destroy();
    }
}

impl Gui {
    pub(crate) fn new(
        winit_window: &Window,
        event_loop: &ActiveEventLoop,
        event_loop_proxy: EventLoopProxy<AppEvent>,
        rendering_context: Rc<OffscreenRenderingContext>,
        // Kept only so callers don't need updating: the address bar this used to
        // seed no longer exists. See CUSTOMIZATIONS.md.
        _initial_url: Url,
    ) -> Self {
        rendering_context
            .make_current()
            .expect("Could not make window RenderingContext current");
        let mut context = EguiGlow::new(
            event_loop,
            rendering_context.glow_gl_api(),
            None,
            None,
            false,
        );

        let mut font_definitions = configure_fonts();
        add_wordmark_font(&mut font_definitions);
        context.egui_ctx.set_fonts(font_definitions);

        context
            .egui_winit
            .init_accesskit(event_loop, winit_window, event_loop_proxy);

        context.egui_ctx.options_mut(|options| {
            // Disable the builtin egui handlers for the Ctrl+Plus, Ctrl+Minus and Ctrl+0
            // shortcuts as they don't work well with servoshell's `device-pixel-ratio` CLI argument.
            options.zoom_with_keyboard = false;

            // On platforms where winit fails to obtain a system theme, fall back to a light theme
            // since it is the more common default.
            options.fallback_theme = egui::Theme::Light;
        });

        let splash_icon_texture = context.egui_ctx.load_texture(
            "boot-splash-icon",
            load_splash_icon_image(),
            egui::TextureOptions::default(),
        );

        let mut gui = Self {
            rendering_context,
            context,
            toolbar_height: Default::default(),
            status_text: None,
            pending_accesskit_updates: vec![],
            splash_icon_texture,
        };

        // Paint one black splash frame *before* the window is ever shown —
        // on every code path, not just a packed-content boot extraction —
        // so there is never a moment where the OS displays the window's
        // undefined/default backing buffer (surfman/the GL driver don't
        // clear it themselves; see CUSTOMIZATIONS.md). `AppState::Booting`
        // repaints over this as extraction proceeds; `AppState::Running`
        // (no pending extraction, or once `BootReady` fires) repaints over
        // it with the real page — which itself clears to
        // `shell_background_color_rgba` (black, see `components/config/
        // prefs.rs`) until it has something of its own to paint.
        // Bracketed with logging (see the milestone-logging comment in
        // `cli::main`) — this is the very first GL draw call/buffer swap
        // this app makes, right after window/context creation, and
        // therefore a plausible place for a silent native crash (GPU
        // driver, ANGLE/GL context issue) that never reaches `panic_hook.rs`.
        log::info!("painting first splash frame");
        gui.update_splash(winit_window, Duration::ZERO);
        gui.paint(winit_window);
        log::info!("painted first splash frame");
        winit_window.set_visible(true);

        gui
    }

    pub(crate) fn has_keyboard_focus(&self) -> bool {
        self.context
            .egui_ctx
            .memory(|memory| memory.focused().is_some())
    }

    pub(crate) fn surrender_focus(&self) {
        self.context.egui_ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
    }

    pub(crate) fn on_window_event(
        &mut self,
        winit_window: &Window,
        event: &WindowEvent,
    ) -> EventResponse {
        self.context.on_window_event(winit_window, event)
    }

    /// The height of the top toolbar of this user inteface ie the distance from the top of the
    /// window to the position of the `WebView`.
    pub(crate) fn toolbar_height(&self) -> Length<f32, DeviceIndependentPixel> {
        self.toolbar_height
    }

    /// Return true iff the given position is over the egui toolbar.
    pub(crate) fn is_in_egui_toolbar_rect(
        &self,
        position: Point2D<f32, DeviceIndependentPixel>,
    ) -> bool {
        position.y < self.toolbar_height.get()
    }

    /// Create a frameless button with square sizing, as used in the toolbar.
    fn toolbar_button(text: &str) -> egui::Button<'_> {
        egui::Button::new(text)
            .frame(false)
            .min_size(Vec2 { x: 20.0, y: 20.0 })
    }

    /// Draws a browser tab, checking for clicks and queues appropriate [`UserInterfaceCommand`]s.
    /// Using a custom widget here would've been nice, but it doesn't seem as though egui
    /// supports that, so we arrange multiple Widgets in a way that they look connected.
    fn browser_tab(
        ui: &mut egui::Ui,
        window: &ServoShellWindow,
        webview: WebView,
        favicon_texture: Option<egui::load::SizedTexture>,
    ) {
        let label = match (webview.page_title(), webview.url()) {
            (Some(title), _) if !title.is_empty() => title,
            (_, Some(url)) => url.to_string(),
            _ => "New Tab".into(),
        };

        let inactive_bg_color = ui.visuals().window_fill;
        let active_bg_color = ui.visuals().widgets.active.weak_bg_fill;
        let active = window.active_webview().map(|webview| webview.id()) == Some(webview.id());

        // Setup a tab frame that will contain the favicon, title and close button
        let mut tab_frame = egui::Frame::NONE.corner_radius(4).begin(ui);
        {
            tab_frame.content_ui.add_space(5.0);

            let visuals = tab_frame.content_ui.visuals_mut();
            // Remove the stroke so we don't see the border between the close button and the label
            visuals.widgets.active.bg_stroke.width = 0.0;
            visuals.widgets.hovered.bg_stroke.width = 0.0;
            // Now we make sure the fill color is always the same, irrespective of state, that way
            // we can make sure that both the label and close button have the same background color
            visuals.widgets.noninteractive.weak_bg_fill = inactive_bg_color;
            visuals.widgets.inactive.weak_bg_fill = inactive_bg_color;
            visuals.widgets.hovered.weak_bg_fill = active_bg_color;
            visuals.widgets.active.weak_bg_fill = active_bg_color;
            visuals.selection.bg_fill = active_bg_color;
            visuals.selection.stroke.color = visuals.widgets.active.fg_stroke.color;
            visuals.widgets.hovered.fg_stroke.color = visuals.widgets.active.fg_stroke.color;

            // Expansion would also show that they are 2 separate widgets
            visuals.widgets.active.expansion = 0.0;
            visuals.widgets.hovered.expansion = 0.0;

            if let Some(favicon) = favicon_texture {
                tab_frame.content_ui.add(
                    egui::Image::from_texture(favicon)
                        .fit_to_exact_size(egui::vec2(16.0, 16.0))
                        .bg_fill(egui::Color32::TRANSPARENT),
                );
            }

            let tab = tab_frame
                .content_ui
                .add(Button::selectable(
                    active,
                    truncate_with_ellipsis(&label, 20),
                ))
                .on_hover_ui(|ui| {
                    ui.label(&label);
                });

            let close_button = tab_frame
                .content_ui
                .add(egui::Button::new("X").fill(egui::Color32::TRANSPARENT));
            close_button.widget_info(|| {
                let mut info = WidgetInfo::new(WidgetType::Button);
                info.label = Some("Close".into());
                info
            });
            if close_button.clicked() || close_button.middle_clicked() || tab.middle_clicked() {
                window
                    .queue_user_interface_command(UserInterfaceCommand::CloseWebView(webview.id()));
            } else if !active && tab.clicked() {
                window.activate_webview(webview.id());
            }
        }

        let response = tab_frame.allocate_space(ui);
        let fill_color = if active || response.hovered() {
            active_bg_color
        } else {
            inactive_bg_color
        };
        tab_frame.frame.fill = fill_color;
        tab_frame.end(ui);
    }

    /// Update the user interface, but do not paint the updated state.
    pub(crate) fn update(
        &mut self,
        state: &RunningAppState,
        window: &ServoShellWindow,
        headed_window: &headed_window::HeadedWindow,
    ) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        let Self {
            rendering_context,
            context,
            toolbar_height,
            ..
        } = self;

        let winit_window = headed_window.winit_window();
        context.run(winit_window, |ctx| {
            // Kiosk/embedded fork: never draw the toolbar or tab strip, in windowed
            // mode or fullscreen — this build is meant to look like a native app
            // window, not a browser.
            *toolbar_height = Length::default();

            let scale =
                Scale::<_, DeviceIndependentPixel, DevicePixel>::new(ctx.pixels_per_point());

            headed_window.for_each_active_dialog(window, |dialog| dialog.update(ctx));

            // If the top parts of the GUI changed size, then update the size of the WebView and also
            // the size of its RenderingContext.
            let available_rect = ctx.available_rect_before_wrap();

            // Build a graft node for each WebView.
            for (webview_id, webview) in window.webviews() {
                if let Some(tree_id) = webview.accesskit_tree_id() {
                    let id = egui::Id::new(webview_id);
                    ctx.accesskit_node_builder(id, |node| {
                        node.set_tree_id(tree_id);
                    });
                }
            }
            let size = Size2D::new(available_rect.width(), available_rect.height()) * scale;
            if let Some(webview) = window.active_webview() &&
                size != webview.size()
            {
                // `rect` is sized to just the WebView viewport, which is required by
                // `OffscreenRenderingContext` See:
                // <https://github.com/servo/servo/issues/38369#issuecomment-3138378527>
                webview.resize(PhysicalSize::new(size.width as u32, size.height as u32))
            }

            if let Some(status_text) = &self.status_text {
                egui::Tooltip::always_open(
                    ctx.clone(),
                    LayerId::new(Order::Tooltip, Id::new("tooltip")),
                    "tooltip layer".into(),
                    pos2(0.0, available_rect.max.y),
                )
                .show(|ui| ui.add(Label::new(status_text.clone()).extend()));
            }

            window.repaint_webviews();

            if let Some(render_to_parent) = rendering_context.render_to_parent_callback() {
                ctx.layer_painter(LayerId::background()).add(PaintCallback {
                    rect: available_rect,
                    callback: Arc::new(CallbackFn::new(move |info, painter| {
                        let clip = info.viewport_in_pixels();
                        let rect_in_parent = Rect::new(
                            Point2D::new(clip.left_px, clip.from_bottom_px),
                            Size2D::new(clip.width_px, clip.height_px),
                        );
                        render_to_parent(painter.gl(), rect_in_parent)
                    })),
                });
            }
        });

        // If any egui widget requested a repaint, also request a repaint for our
        // containing window. This allows egui widget to animate on their own.
        if self.context.egui_ctx.has_requested_repaint() {
            window.set_needs_repaint();
        }

        let adapter = self
            .context
            .egui_winit
            .accesskit
            .as_mut()
            .expect("guaranteed by Gui::new()");
        for tree_update in self.pending_accesskit_updates.drain(..) {
            adapter.update_if_active(|| tree_update);
        }
    }

    /// Update the boot splash — a minimal black screen with the Roves icon and wordmark
    /// (see `resources/roves_wordmark.svg` for the same lockup as a standalone asset),
    /// shown in place of the normal browser UI both while a packed-content launch's boot
    /// extraction still hasn't finished (`AppState::Booting` in `app.rs`) and while the
    /// real page that follows it is still loading (`headed_window.rs`'s
    /// `page_load_splash_since`). `elapsed` drives a squared-off, indeterminate
    /// progress-bar animation below the wordmark (see [`draw_splash_progress_bar`] for
    /// why it's indeterminate rather than a completion fraction) — always drawn, so the
    /// splash never shows a bare wordmark with no indication that something is loading.
    /// Call [`Gui::paint`] afterward, same as [`Gui::update`].
    pub(crate) fn update_splash(&mut self, winit_window: &Window, elapsed: Duration) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        let wordmark_font = egui::FontId::new(
            SPLASH_WORDMARK_FONT_SIZE,
            egui::FontFamily::Name("Metal Mania".into()),
        );
        // Cloned (cheap — `TextureHandle` is a small ref-counted handle) so the closure
        // below doesn't need to borrow `self` at all: `self.context.run` already holds
        // `self.context` mutably for the duration of the closure, and — separately, the
        // actual reason this has to happen out here rather than being a borrow-checker
        // nicety — `egui::Context::fonts_mut` (needed inside, to measure the wordmark)
        // panics with "No fonts available until first call to Context::run()" if called
        // any earlier than the closure itself; confirmed the hard way, on a real build.
        let splash_icon_texture = self.splash_icon_texture.clone();
        self.context.run(winit_window, |ctx| {
            // Measured (not guessed) — both so the icon+wordmark row below can be
            // centered exactly, rather than trusting `top_down`'s `Align::Center` to
            // center a nested `ui.horizontal` row on its own, and so the icon can be
            // sized to actually match the wordmark's rendered height instead of an
            // independently guessed constant (`SPLASH_ICON_SIZE` used to be hardcoded
            // to 128px against an 88px font size — a ratio borrowed from
            // `resources/roves_wordmark.svg`'s lockup that doesn't necessarily hold for
            // Metal Mania's actual glyph metrics at this size).
            let wordmark_size = ctx.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(
                        "Roves".to_owned(),
                        wordmark_font.clone(),
                        egui::Color32::WHITE,
                    )
                    .size()
            });
            // See `SPLASH_ICON_CONTENT_HEIGHT_RATIO`'s own doc comment: the icon's
            // *texture* is sized taller than the wordmark so the visible badge inside
            // its transparent padding ends up the intended height, not the padded
            // canvas. Every other measurement below that used to treat "the icon's
            // height" as `wordmark_size.y` — the row's vertical centering, the lockup
            // width — needs this same compensated value instead, now that the two
            // aren't equal any more.
            let icon_size = wordmark_size.y / SPLASH_ICON_CONTENT_HEIGHT_RATIO * SPLASH_ICON_SCALE;
            // `.max_height()` only ever *caps* a size, it never scales an image up past
            // its own default/native size — confirmed directly against egui's own docs
            // after a real build showed `icon_size` growing (per `SPLASH_ICON_SCALE`
            // above) with zero visible effect on the rendered icon, while the layout
            // math below (which does use `icon_size`) shifted everything else out of
            // alignment to compensate for a size change that was never actually
            // rendered. `fit_to_exact_size` forces the actual displayed size instead —
            // `resources/servo_64.png`'s canvas is square, so `Vec2::splat` is correct
            // here without distorting the (also square) source texture.
            let icon = egui::Image::from_texture(&splash_icon_texture)
                .fit_to_exact_size(egui::Vec2::splat(icon_size));
            // `Panel::show` (the top-level entry point, as opposed to
            // `show_inside` for nesting inside another container) is
            // deprecated in this egui version in favor of hand-building a
            // full-window `Ui` — not worth the extra internal-API surface
            // for this deliberately simple splash.
            #[expect(deprecated)]
            egui::CentralPanel::default()
                .frame(egui::Frame::default().fill(egui::Color32::BLACK))
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        // Half-height offset for the icon+wordmark row (`icon_size` now
                        // dominates it, being taller than the wordmark) + gap (40px) +
                        // progress bar (`SPLASH_PROGRESS_BAR_HEIGHT`).
                        ui.add_space(
                            ui.available_height() / 2.0
                                - (icon_size + 40.0 + SPLASH_PROGRESS_BAR_HEIGHT) / 2.0,
                        );
                        let lockup_width =
                            icon_size + ui.spacing().item_spacing.x + wordmark_size.x;
                        ui.horizontal(|ui| {
                            ui.add_space(((ui.available_width() - lockup_width) / 2.0).max(0.0));
                            ui.add(icon.clone());
                            ui.label(
                                egui::RichText::new("Roves")
                                    .font(wordmark_font.clone())
                                    .color(egui::Color32::WHITE),
                            );
                        });
                        ui.add_space(40.0);
                        draw_splash_progress_bar(ui, elapsed);
                    });
                });
        });
    }

    /// Paint the GUI, as of the last update.
    pub(crate) fn paint(&mut self, window: &Window) {
        self.rendering_context
            .make_current()
            .expect("Could not make RenderingContext current");
        self.rendering_context
            .parent_context()
            .prepare_for_rendering();
        self.context.paint(window);
        self.rendering_context.parent_context().present();
    }

    fn update_status_text(&mut self, window: &ServoShellWindow) -> bool {
        let state_status = window
            .active_webview()
            .and_then(|webview| webview.status_text());
        let old_status = std::mem::replace(&mut self.status_text, state_status);
        old_status != self.status_text
    }

    /// Updates all fields taken from the given [`ServoShellWindow`]. Returns true iff the egui
    /// needs an update.
    pub(crate) fn update_webview_data(&mut self, window: &ServoShellWindow) -> bool {
        self.update_status_text(window)
    }

    /// Returns true if a redraw is required after handling the provided event.
    pub(crate) fn handle_accesskit_event(
        &mut self,
        event: &egui_winit::accesskit_winit::WindowEvent,
    ) -> bool {
        match event {
            egui_winit::accesskit_winit::WindowEvent::InitialTreeRequested => {
                self.context.egui_ctx.enable_accesskit();
                true
            },
            egui_winit::accesskit_winit::WindowEvent::ActionRequested(req) => {
                self.context
                    .egui_winit
                    .on_accesskit_action_request(req.clone());
                true
            },
            egui_winit::accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                self.context.egui_ctx.disable_accesskit();
                false
            },
        }
    }

    pub(crate) fn set_zoom_factor(&self, factor: f32) {
        self.context.egui_ctx.set_zoom_factor(factor);
    }

    pub(crate) fn notify_accessibility_tree_update(&mut self, tree_update: accesskit::TreeUpdate) {
        self.pending_accesskit_updates.push(tree_update);
    }
}
