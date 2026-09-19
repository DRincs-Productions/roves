/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

/// Log an event from the SDL3-backed event loop (see `desktop/event_loop.rs`) at trace level.
/// - To disable tracing: RUST_LOG='servoshell<winit@=off'
/// - To enable tracing: RUST_LOG='servoshell<winit@'
///
/// TODO(SDL3 windowing): the `<winit@` log-target prefix is kept as-is rather than renamed to
/// `<sdl3@` purely to avoid a drive-by rename of every filter example above and in any
/// downstream docs/scripts that already reference it -- revisit once the SDL3 migration is far
/// enough along that "winit" reads as actively misleading rather than just a legacy name.
macro_rules! trace_winit_event {
    // This macro only exists to put the docs in the same file as the target prefix,
    // so the macro definition is always the same.
    ($event:expr, $($rest:tt)+) => {
        ::log::trace!(target: $crate::desktop::tracing::LogTarget::log_target(&$event), $($rest)+)
    };
}

pub(crate) use trace_winit_event;

/// Get the log target for an event, as a static string.
pub(crate) trait LogTarget {
    fn log_target(&self) -> &'static str;
}

mod from_sdl3 {
    use super::LogTarget;
    use crate::desktop::event_loop::{AppEvent, WindowEvent};

    macro_rules! target {
        ($($name:literal)+) => {
            concat!("servoshell<winit@", $($name),+)
        };
    }

    impl LogTarget for AppEvent {
        fn log_target(&self) -> &'static str {
            match self {
                Self::Waker => target!("UserEvent(Waker)"),
                Self::RedrawRequested(..) => target!("RedrawRequested"),
                Self::CloseAllWindows => target!("UserEvent(CloseAllWindows)"),
                Self::BootProgress(..) => target!("UserEvent(BootProgress)"),
                Self::BootReady => target!("UserEvent(BootReady)"),
                Self::SaveFileDialog { .. } => target!("UserEvent(SaveFileDialog)"),
            }
        }
    }

    impl LogTarget for WindowEvent {
        fn log_target(&self) -> &'static str {
            match self {
                Self::Resized(..) => target!("WindowEvent(Resized)"),
                Self::CloseRequested => target!("WindowEvent(CloseRequested)"),
                Self::RedrawRequested => target!("RedrawRequested"),
                Self::Focused(..) => target!("WindowEvent(Focused)"),
                Self::KeyDown { .. } => target!("WindowEvent(KeyDown)"),
                Self::KeyUp { .. } => target!("WindowEvent(KeyUp)"),
                Self::MouseMotion { .. } => target!("WindowEvent(MouseMotion)"),
                Self::MouseButtonDown { .. } => target!("WindowEvent(MouseButtonDown)"),
                Self::MouseButtonUp { .. } => target!("WindowEvent(MouseButtonUp)"),
                Self::MouseWheel { .. } => target!("WindowEvent(MouseWheel)"),
                Self::CursorLeft => target!("WindowEvent(CursorLeft)"),
            }
        }
    }
}
