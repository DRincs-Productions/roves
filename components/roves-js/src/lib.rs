/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The subset of the `mozjs` API that Servo's script crates use, implemented on V8.
//!
//! The V8 cutover swaps the workspace `js` dependency from `mozjs` to this crate, so the DOM
//! code keeps its `js::...` paths. `support/v8_cutover_check.py` measures how far the
//! workspace is from compiling against it. Modules mirror mozjs's layout.
