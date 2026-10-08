/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rooting and tracing (mozjs's `gc` module). See `root` for the root-stack model; the other
//! submodules are ported from mozjs/mozjs_sys onto it.

mod collections;
mod custom;
mod root;
mod rooted_traceables;
mod traceable;

pub use collections::*;
pub use custom::*;
pub use root::*;
pub use crate::jsapi::StackGCVector;
pub use rooted_traceables::*;
pub use traceable::Traceable;
