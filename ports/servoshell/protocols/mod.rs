/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Protocol handlers shared between the desktop shell (`desktop/protocols/`, which has its
//! own additional desktop-only handlers alongside these) and the Android EGL
//! shell (`egl/app.rs`) -- unlike `desktop/protocols/`, this module isn't gated to any
//! specific target. Moved out of `desktop/protocols/` (2026-09-12) so both the `file:`
//! rebasing fix and the full `game:` virtual-origin protocol could be reused on Android
//! instead of only ever existing for desktop -- see CUSTOMIZATIONS.md's 2026-09-12 entries
//! for the two real-device bugs this was ported to fix (root-absolute asset references, and
//! a client-side router's own `location.pathname` matching at boot).

pub(crate) mod file;
pub(crate) mod game;
pub(crate) mod packed_content;
