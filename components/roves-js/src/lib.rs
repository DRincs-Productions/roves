/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The subset of the `mozjs` API that Servo's script crates use, implemented on V8.
//!
//! The V8 cutover swaps the workspace `js` dependency from `mozjs` to this crate, so the DOM
//! code keeps its `js::...` paths. `support/v8_cutover_check.py` measures how far the
//! workspace is from compiling against it. Modules mirror mozjs's layout; see
//! `docs/V8_MIGRATION.md` ("Cutover strategy") for the value and rooting model.

#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]

mod cell;
pub mod context;
pub mod gc;
pub mod jsapi;
pub mod jsval;
pub mod rust;

/// Roots a value on this thread's root stack for the enclosing scope (mozjs's `rooted!`).
#[macro_export]
macro_rules! rooted {
    (&in($cx:expr) $($t:tt)*) => {
        $crate::rooted!(in(unsafe { $cx.raw_cx_no_gc() }) $($t)*);
    };
    (in($cx:expr) let $($var:ident)+ = $init:expr) => {
        let mut __root = ::std::mem::MaybeUninit::uninit();
        let $($var)+ = $crate::gc::RootedGuard::new($cx, &mut __root, $init);
    };
    (in($cx:expr) let $($var:ident)+: $type:ty = $init:expr) => {
        let mut __root = ::std::mem::MaybeUninit::uninit();
        let $($var)+: $crate::gc::RootedGuard<$type> = $crate::gc::RootedGuard::new($cx, &mut __root, $init);
    };
    (in($cx:expr) let $($var:ident)+: $type:ty) => {
        let mut __root = ::std::mem::MaybeUninit::uninit();
        // SAFETY: the initial value is immediately stored in a rooted location.
        let $($var)+: $crate::gc::RootedGuard<$type> = $crate::gc::RootedGuard::new(
            $cx,
            &mut __root,
            unsafe { <$type as $crate::gc::GCMethods>::initial() },
        );
    };
}

#[cfg(test)]
mod tests;
