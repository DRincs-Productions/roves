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

mod api;
mod binding;
mod cell;
pub mod context;
pub mod conversions;
pub mod error;
pub mod gc;
pub mod glue;
pub mod jsapi;
pub mod jsid;
pub mod jsval;
mod jsimpls;
mod jsapi_impl;
mod native;
mod object;
mod proxy;
mod runtime_impl;
mod typedarray_impl;
pub mod typedarray;
mod realm_impl;
pub mod realm;
pub mod panic;
pub mod rust;

pub use crate::jsval::{JS_ARGV, JS_CALLEE};
pub use crate::object::{
    JSCLASS_FOREGROUND_FINALIZE, JSCLASS_GLOBAL_SLOT_COUNT, JSCLASS_IS_DOMJSCLASS, JSCLASS_IS_GLOBAL,
    JSCLASS_IS_PROXY, JSCLASS_RESERVED_SLOTS_MASK, JSCLASS_RESERVED_SLOTS_SHIFT, JSCLASS_USERBIT1,
};

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

/// Roots a custom-traceable value for the enclosing scope (mozjs's `auto_root!`).
#[macro_export]
macro_rules! auto_root {
    (&in($cx:expr) $($t:tt)*) => {
        $crate::auto_root!(in(unsafe { $cx.raw_cx_no_gc() }) $($t)*);
    };
    (in($cx:expr) let $($var:ident)+ = $init:expr) => {
        let mut __root = $crate::gc::CustomAutoRooter::new($init);
        let $($var)+ = __root.root($cx);
    };
    (in($cx:expr) let $($var:ident)+: $type:ty = $init:expr) => {
        let mut __root = $crate::gc::CustomAutoRooter::new($init);
        let $($var)+: $crate::gc::CustomAutoRooterGuard<$type> = __root.root($cx);
    };
}

/// A vector rooted for the enclosing scope (mozjs's `rooted_vec!`).
#[macro_export]
macro_rules! rooted_vec {
    (let mut $name:ident) => {
        let mut __root = $crate::gc::RootableVec::new_unrooted();
        let mut $name = $crate::gc::RootedVec::new(&mut __root);
    };
    (let $name:ident <- $iter:expr) => {
        let mut __root = $crate::gc::RootableVec::new_unrooted();
        let $name = $crate::gc::RootedVec::from_iter(&mut __root, $iter);
    };
    (let mut $name:ident <- $iter:expr) => {
        let mut __root = $crate::gc::RootableVec::new_unrooted();
        let mut $name = $crate::gc::RootedVec::from_iter(&mut __root, $iter);
    };
}

/// Packs a `JSJitInfo`'s first bitfield (ported from mozjs 0.21.6; the generated bindings
/// use it in every getter/setter/method info).
#[macro_export]
macro_rules! new_jsjitinfo_bitfield_1 {
    (
        $type_: expr,
        $aliasSet_: expr,
        $returnType_: expr,
        $isInfallible: expr,
        $isMovable: expr,
        $isEliminatable: expr,
        $isAlwaysInSlot: expr,
        $isLazilyCachedInSlot: expr,
        $isTypedMethod: expr,
        $slotIndex: expr,
    ) => {
        0 | (($type_ as u32) << 0u32)
            | (($aliasSet_ as u32) << 4u32)
            | (($returnType_ as u32) << 8u32)
            | (($isInfallible as u32) << 16u32)
            | (($isMovable as u32) << 17u32)
            | (($isEliminatable as u32) << 18u32)
            | (($isAlwaysInSlot as u32) << 19u32)
            | (($isLazilyCachedInSlot as u32) << 20u32)
            | (($isTypedMethod as u32) << 21u32)
            | (($slotIndex as u32) << 22u32)
    };
}

#[cfg(test)]
mod tests;
