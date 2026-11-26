// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Main-frame navigation veto (CEF `OnBeforeBrowse`).
//!
//! The host sets [`crate::CefSurfaceConfig::main_frame_navigation_policy`]; for
//! every main-frame navigation CEF asks before it starts (host `navigate_to_url`
//! calls, link clicks, `location` assignments, reloads, server redirects) the
//! policy decides. A refused navigation is cancelled before any request is sent
//! and reported as [`crate::NavigationEvent::NavigationBlocked`]. Sub-frame
//! navigations are not asked about.
//!
//! Cancelling in `OnBeforeBrowse` makes Chromium abort the navigation, which CEF
//! may also report through `OnLoadError` with `ERR_ABORTED`. That error is the
//! veto's own echo, not a page failure, so the load handler drops it.

use std::sync::{Arc, Mutex};

/// Decides, synchronously on CEF's UI thread, whether a main-frame navigation
/// to the given URL may proceed (`true`) or is cancelled (`false`).
///
/// It runs inside a CEF callback: keep it cheap, never block, and never call
/// back into the producer from it.
pub type MainFrameNavigationPolicy = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Chromium's `net::ERR_ABORTED`.
const ERR_ABORTED: i32 = -3;

#[derive(Default)]
pub(crate) struct NavigationGuard {
    policy: Option<MainFrameNavigationPolicy>,
    /// The last vetoed URL whose `ERR_ABORTED` echo has not been seen yet.
    vetoed: Mutex<Option<String>>,
}

impl NavigationGuard {
    pub(crate) fn new(policy: Option<MainFrameNavigationPolicy>) -> Self {
        Self {
            policy,
            vetoed: Mutex::new(None),
        }
    }

    /// Whether a main-frame navigation to `url` must be cancelled. With no
    /// policy every navigation proceeds, as before this hook existed.
    pub(crate) fn veto(&self, url: &str) -> bool {
        let Some(policy) = self.policy.as_ref() else {
            return false;
        };
        if policy(url) {
            return false;
        }
        *self.vetoed.lock().unwrap_or_else(|e| e.into_inner()) = Some(url.to_owned());
        true
    }

    /// Whether a main-frame load error is the abort a veto caused (consumed once).
    pub(crate) fn is_veto_echo(&self, url: &str, error_code: i32) -> bool {
        if error_code != ERR_ABORTED {
            return false;
        }
        let mut vetoed = self.vetoed.lock().unwrap_or_else(|e| e.into_inner());
        if vetoed.as_deref() == Some(url) {
            *vetoed = None;
            true
        } else {
            false
        }
    }
}

/// The shared body of every backend's `RequestHandler::on_before_browse`:
/// `Some(event)` when the navigation is cancelled (the caller pushes the event
/// and returns 1), `None` when it proceeds.
#[cfg(feature = "cef-runtime")]
pub(crate) fn before_browse(
    guard: &NavigationGuard,
    frame: Option<&mut cef::Frame>,
    request: Option<&mut cef::Request>,
    user_gesture: ::std::os::raw::c_int,
    is_redirect: ::std::os::raw::c_int,
) -> Option<crate::surface::NavigationEvent> {
    use cef::{ImplFrame, ImplRequest};
    guard.policy.as_ref()?;
    if !frame.is_some_and(|f| f.is_main() != 0) {
        return None;
    }
    let url = cef::CefString::from(&request?.url()).to_string();
    if !guard.veto(&url) {
        return None;
    }
    log::debug!("weld: main-frame navigation vetoed");
    Some(crate::surface::NavigationEvent::NavigationBlocked {
        url,
        user_gesture: user_gesture != 0,
        is_redirect: is_redirect != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_policy_never_vetoes() {
        let guard = NavigationGuard::new(None);
        assert!(!guard.veto("https://example.com/"));
        assert!(!guard.is_veto_echo("https://example.com/", ERR_ABORTED));
    }

    #[test]
    fn policy_decides_and_the_abort_echo_is_consumed_once() {
        let guard = NavigationGuard::new(Some(Arc::new(|url: &str| {
            url.starts_with("http://127.0.0.1:1/")
        })));
        assert!(!guard.veto("http://127.0.0.1:1/index.html"));
        assert!(guard.veto("https://example.com/"));
        // Another code or URL is a real failure.
        assert!(!guard.is_veto_echo("https://example.com/", -2));
        assert!(!guard.is_veto_echo("https://other.example/", ERR_ABORTED));
        assert!(guard.is_veto_echo("https://example.com/", ERR_ABORTED));
        assert!(!guard.is_veto_echo("https://example.com/", ERR_ABORTED));
    }
}
