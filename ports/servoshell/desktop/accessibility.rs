/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! AccessKit's platform adapters do not depend on winit. This module supplies the small SDL3
//! shell which `accesskit_winit` used to provide: native-handle construction, focus/bounds event
//! forwarding, and thread-safe delivery of activation/action/deactivation requests.

use std::sync::mpsc::{self, Receiver, Sender};

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate,
};

use crate::desktop::event_loop::{AppEvent, EventLoopProxy, WindowEvent, WindowId};

#[derive(Debug)]
pub(crate) enum AccessibilityEvent {
    InitialTreeRequested,
    ActionRequested(ActionRequest),
    Deactivated,
}

#[derive(Clone)]
struct EventSender {
    sender: Sender<AccessibilityEvent>,
    event_loop_proxy: EventLoopProxy,
    window_id: WindowId,
}

impl EventSender {
    fn send(&self, event: AccessibilityEvent) {
        self.sender.send(event).ok();
        self.event_loop_proxy
            .send_event(AppEvent::RedrawRequested(self.window_id))
            .ok();
    }
}

impl ActivationHandler for EventSender {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.send(AccessibilityEvent::InitialTreeRequested);
        None
    }
}

impl ActionHandler for EventSender {
    fn do_action(&mut self, request: ActionRequest) {
        self.send(AccessibilityEvent::ActionRequested(request));
    }
}

impl DeactivationHandler for EventSender {
    fn deactivate_accessibility(&mut self) {
        self.send(AccessibilityEvent::Deactivated);
    }
}

pub(crate) struct SdlAccessKit {
    adapter: platform::Adapter,
    events: Receiver<AccessibilityEvent>,
}

impl SdlAccessKit {
    pub(crate) fn new(
        window: &sdl3::video::Window,
        event_loop_proxy: EventLoopProxy,
    ) -> Self {
        let (sender, events) = mpsc::channel();
        let sender = EventSender {
            sender,
            event_loop_proxy,
            window_id: window.id(),
        };
        let adapter = platform::Adapter::new(
            window,
            sender.clone(),
            sender.clone(),
            sender,
        );
        Self { adapter, events }
    }

    pub(crate) fn process_window_event(
        &mut self,
        window: &sdl3::video::Window,
        event: &WindowEvent,
    ) {
        self.adapter.process_window_event(window, event);
    }

    pub(crate) fn update_if_active(&mut self, update: TreeUpdate) {
        self.adapter.update_if_active(|| update);
    }

    pub(crate) fn drain_events(&mut self) -> impl Iterator<Item = AccessibilityEvent> + '_ {
        self.events.try_iter()
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use accesskit_windows::{HWND, SubclassingAdapter};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use crate::desktop::event_loop::WindowEvent;

    pub(super) struct Adapter(SubclassingAdapter);

    impl Adapter {
        pub(super) fn new(
            window: &sdl3::video::Window,
            activation: impl 'static + ActivationHandler,
            action: impl 'static + ActionHandler + Send,
            _deactivation: impl 'static + DeactivationHandler,
        ) -> Self {
            let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
                unreachable!("SDL3 returned a non-Win32 handle on Windows")
            };
            Self(SubclassingAdapter::new(
                HWND(handle.hwnd.get() as *mut _),
                activation,
                action,
            ))
        }

        pub(super) fn process_window_event(
            &mut self,
            _window: &sdl3::video::Window,
            _event: &WindowEvent,
        ) {
        }

        pub(super) fn update_if_active(&mut self, update: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.0.update_if_active(update) {
                events.raise();
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use accesskit_macos::SubclassingAdapter;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use crate::desktop::event_loop::WindowEvent;

    pub(super) struct Adapter(SubclassingAdapter);

    impl Adapter {
        pub(super) fn new(
            window: &sdl3::video::Window,
            activation: impl 'static + ActivationHandler,
            action: impl 'static + ActionHandler,
            _deactivation: impl 'static + DeactivationHandler,
        ) -> Self {
            let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw() else {
                unreachable!("SDL3 returned a non-AppKit handle on macOS")
            };
            Self(unsafe { SubclassingAdapter::new(handle.ns_view.as_ptr(), activation, action) })
        }

        pub(super) fn process_window_event(
            &mut self,
            _window: &sdl3::video::Window,
            event: &WindowEvent,
        ) {
            if let WindowEvent::Focused(focused) = event &&
                let Some(events) = self.0.update_view_focus_state(*focused)
            {
                events.raise();
            }
        }

        pub(super) fn update_if_active(&mut self, update: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.0.update_if_active(update) {
                events.raise();
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, Rect, TreeUpdate};
    use accesskit_unix::Adapter as UnixAdapter;

    use crate::desktop::event_loop::WindowEvent;

    pub(super) struct Adapter(UnixAdapter);

    impl Adapter {
        pub(super) fn new(
            window: &sdl3::video::Window,
            activation: impl 'static + ActivationHandler + Send,
            action: impl 'static + ActionHandler + Send,
            deactivation: impl 'static + DeactivationHandler + Send,
        ) -> Self {
            let mut adapter = Self(UnixAdapter::new(activation, action, deactivation));
            adapter.update_bounds(window);
            adapter
        }

        fn update_bounds(&mut self, window: &sdl3::video::Window) {
            let (x, y) = window.position();
            let (width, height) = window.size();
            let bounds = Rect::from_origin_size(
                (f64::from(x), f64::from(y)),
                (f64::from(width), f64::from(height)),
            );
            self.0.set_root_window_bounds(bounds, bounds);
        }

        pub(super) fn process_window_event(
            &mut self,
            window: &sdl3::video::Window,
            event: &WindowEvent,
        ) {
            match event {
                WindowEvent::Resized(..) => self.update_bounds(window),
                WindowEvent::Focused(focused) => self.0.update_window_focus_state(*focused),
                _ => {},
            }
        }

        pub(super) fn update_if_active(&mut self, update: impl FnOnce() -> TreeUpdate) {
            self.0.update_if_active(update);
        }
    }
}
