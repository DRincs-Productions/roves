/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// `file`/`game`/`packed_content` moved out to `crate::protocols` (2026-09-12) so the
// Android EGL shell (`egl/app.rs`) could reuse them too -- see that module's own
// doc comment. Everything below here is still desktop-only.
pub(crate) mod resource;
pub(crate) mod roves;
pub(crate) mod saves;
pub(crate) mod servo;
#[cfg(feature = "steam")]
pub(crate) mod steam;
pub(crate) mod urlinfo;
