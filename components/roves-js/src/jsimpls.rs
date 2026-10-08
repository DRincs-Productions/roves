/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Ported from mozjs_sys 140.14 (`src/jsimpls.rs`, same license): behaviour of the bindgen
// types, which roves-js copies with their layouts.

use crate::jsapi::*;
use crate::jsval::{JSVal, UndefinedValue};

use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr;

impl<T> Deref for Handle<T> {
    type Target = T;

    fn deref<'a>(&'a self) -> &'a T {
        unsafe { &*self.ptr }
    }
}

impl<T> Deref for MutableHandle<T> {
    type Target = T;

    fn deref<'a>(&'a self) -> &'a T {
        unsafe { &*self.ptr }
    }
}

impl Default for PropertyDescriptor {
    fn default() -> Self {
        PropertyDescriptor {
            _bitfield_align_1: [],
            _bitfield_1: Default::default(),
            getter_: ptr::null_mut(),
            setter_: ptr::null_mut(),
            value_: UndefinedValue(),
        }
    }
}

impl Drop for JSAutoRealm {
    fn drop(&mut self) {
        unsafe {
            LeaveRealm(self.cx_, self.oldRealm_);
        }
    }
}

impl<T> Handle<T> {
    pub fn get(&self) -> T
    where
        T: Copy,
    {
        unsafe { *self.ptr }
    }

    pub unsafe fn from_marked_location(ptr: *const T) -> Handle<T> {
        Handle {
            ptr: ptr as *mut T,
            _phantom_0: PhantomData,
        }
    }
}

impl<T> MutableHandle<T> {
    pub unsafe fn from_marked_location(ptr: *mut T) -> MutableHandle<T> {
        MutableHandle {
            ptr,
            _phantom_0: PhantomData,
        }
    }

    pub fn handle(&self) -> Handle<T> {
        unsafe { Handle::from_marked_location(self.ptr as *const _) }
    }

    pub fn get(&self) -> T
    where
        T: Copy,
    {
        unsafe { *self.ptr }
    }

    pub fn set(&self, v: T)
    where
        T: Copy,
    {
        unsafe { *self.ptr = v }
    }

    /// The returned pointer is aliased by a pointer that the GC will read
    /// through, and thus `&mut` references created from it must not be held
    /// across GC pauses.
    pub fn as_ptr(self) -> *mut T {
        self.ptr
    }
}

impl HandleValue {
    pub fn null() -> HandleValue {
        unsafe { NullHandleValue }
    }

    pub fn undefined() -> HandleValue {
        unsafe { UndefinedHandleValue }
    }
}

impl HandleValueArray {
    pub fn empty() -> HandleValueArray {
        HandleValueArray {
            length_: 0,
            elements_: ptr::null(),
        }
    }
}

impl From<&CallArgs> for HandleValueArray {
    fn from(args: &CallArgs) -> HandleValueArray {
        HandleValueArray {
            length_: args.argc_ as usize,
            elements_: args.argv_ as *const _,
        }
    }
}

impl From<Handle<JSVal>> for HandleValueArray {
    fn from(handle: Handle<JSVal>) -> HandleValueArray {
        HandleValueArray {
            length_: 1,
            elements_: handle.ptr,
        }
    }
}

const NULL_OBJECT: *mut JSObject = 0 as *mut JSObject;

impl HandleObject {
    pub fn null() -> HandleObject {
        unsafe { HandleObject::from_marked_location(&NULL_OBJECT) }
    }
}

// ___________________________________________________________________________
// Implementations for various things in jsapi.rs

impl JSAutoRealm {
    pub fn new(cx: *mut JSContext, target: *mut JSObject) -> JSAutoRealm {
        JSAutoRealm {
            cx_: cx,
            oldRealm_: unsafe { EnterRealm(cx, target) },
        }
    }
}

impl JSJitMethodCallArgs {
    #[inline]
    pub fn get(&self, i: u32) -> HandleValue {
        unsafe {
            if i < self.argc_ {
                HandleValue::from_marked_location(self.argv_.offset(i as isize))
            } else {
                UndefinedHandleValue
            }
        }
    }

    #[inline]
    pub fn index(&self, i: u32) -> HandleValue {
        assert!(i < self.argc_);
        unsafe { HandleValue::from_marked_location(self.argv_.offset(i as isize)) }
    }

    #[inline]
    pub fn index_mut(&self, i: u32) -> MutableHandleValue {
        assert!(i < self.argc_);
        unsafe { MutableHandleValue::from_marked_location(self.argv_.offset(i as isize)) }
    }

    #[inline]
    pub fn rval(&self) -> MutableHandleValue {
        unsafe { MutableHandleValue::from_marked_location(self.argv_.offset(-2)) }
    }
}

impl JSJitGetterCallArgs {
    #[inline]
    pub fn rval(&self) -> MutableHandleValue {
        self._base
    }
}

// XXX need to hack up bindgen to convert this better so we don't have
//     to duplicate so much code here
impl CallArgs {
    #[inline]
    pub unsafe fn from_vp(vp: *mut Value, argc: u32) -> CallArgs {
        // For some reason, with debugmozjs, calling
        // JS_CallArgsFromVp(argc, vp)
        // produces a SEGV caused by the vp being overwritten by the argc.
        // TODO: debug this!
        CallArgs {
            _bitfield_align_1: Default::default(),
            _bitfield_1: CallArgs::new_bitfield_1((*vp.offset(1)).is_magic(), false),
            argc_: argc,
            argv_: vp.offset(2),
            __bindgen_padding_0: [0, 0, 0],
        }
    }

    #[inline]
    pub fn index(&self, i: u32) -> HandleValue {
        assert!(i < self.argc_);
        unsafe { HandleValue::from_marked_location(self.argv_.offset(i as isize)) }
    }

    #[inline]
    pub fn index_mut(&self, i: u32) -> MutableHandleValue {
        assert!(i < self.argc_);
        unsafe { MutableHandleValue::from_marked_location(self.argv_.offset(i as isize)) }
    }

    #[inline]
    pub fn get(&self, i: u32) -> HandleValue {
        unsafe {
            if i < self.argc_ {
                HandleValue::from_marked_location(self.argv_.offset(i as isize))
            } else {
                UndefinedHandleValue
            }
        }
    }

    #[inline]
    pub fn rval(&self) -> MutableHandleValue {
        unsafe { MutableHandleValue::from_marked_location(self.argv_.offset(-2)) }
    }

    #[inline]
    pub fn thisv(&self) -> HandleValue {
        unsafe { HandleValue::from_marked_location(self.argv_.offset(-1)) }
    }

    #[inline]
    pub fn calleev(&self) -> HandleValue {
        unsafe { HandleValue::from_marked_location(self.argv_.offset(-2)) }
    }

    #[inline]
    pub fn callee(&self) -> *mut JSObject {
        self.calleev().to_object()
    }

    #[inline]
    pub fn new_target(&self) -> MutableHandleValue {
        assert!(self.constructing_());
        unsafe {
            MutableHandleValue::from_marked_location(self.argv_.offset(self.argc_ as isize))
        }
    }

    #[inline]
    pub fn is_constructing(&self) -> bool {
        unsafe { (*self.argv_.offset(-1)).is_magic() }
    }
}

impl JSJitSetterCallArgs {
    #[inline]
    pub fn get(&self, i: u32) -> HandleValue {
        assert!(i == 0);
        self._base.handle()
    }
}

impl JSFunctionSpec {
    pub const ZERO: Self = JSFunctionSpec {
        name: JSPropertySpec_Name {
            string_: ptr::null(),
        },
        selfHostedName: 0 as *const _,
        flags: 0,
        nargs: 0,
        call: JSNativeWrapper::ZERO,
    };

    pub fn is_zeroed(&self) -> bool {
        (unsafe { self.name.string_.is_null() })
            && self.selfHostedName.is_null()
            && self.flags == 0
            && self.nargs == 0
            && self.call.is_zeroed()
    }
}

impl JSPropertySpec {
    pub const ZERO: Self = JSPropertySpec {
        name: JSPropertySpec_Name {
            string_: ptr::null(),
        },
        attributes_: 0,
        kind_: JSPropertySpec_Kind::NativeAccessor,
        u: JSPropertySpec_AccessorsOrValue {
            accessors: JSPropertySpec_AccessorsOrValue_Accessors {
                getter: JSPropertySpec_Accessor {
                    native: JSNativeWrapper::ZERO,
                },
                setter: JSPropertySpec_Accessor {
                    native: JSNativeWrapper::ZERO,
                },
            },
        },
    };

    /// https://searchfox.org/mozilla-central/rev/2bdaa395cb841b28f8ef74882a61df5efeedb42b/js/public/PropertySpec.h#305-307
    pub fn is_accessor(&self) -> bool {
        self.kind_ == JSPropertySpec_Kind::NativeAccessor
            || self.kind_ == JSPropertySpec_Kind::SelfHostedAccessor
    }

    pub fn is_zeroed(&self) -> bool {
        (unsafe { self.name.string_.is_null() })
            && self.attributes_ == 0
            && self.is_accessor()
            && unsafe { self.u.accessors.getter.native.is_zeroed() }
            && unsafe { self.u.accessors.setter.native.is_zeroed() }
    }
}

impl JSNativeWrapper {
    pub const ZERO: Self = JSNativeWrapper {
        info: 0 as *const _,
        op: None,
    };

    pub fn is_zeroed(&self) -> bool {
        self.op.is_none() && self.info.is_null()
    }
}

impl ObjectOpResult {
    pub fn ok(&self) -> bool {
        assert_ne!(
            self.code_,
            ObjectOpResult_SpecialCodes::Uninitialized as usize
        );
        self.code_ == ObjectOpResult_SpecialCodes::OkCode as usize
    }

    /// Set this ObjectOpResult to true and return true.
    pub fn succeed(&mut self) -> bool {
        self.code_ = ObjectOpResult_SpecialCodes::OkCode as usize;
        true
    }

    pub fn fail(&mut self, code: JSErrNum) -> bool {
        assert_ne!(
            code as usize,
            ObjectOpResult_SpecialCodes::OkCode as usize
        );
        self.code_ = code as usize;
        true
    }

    pub fn fail_cant_redefine_prop(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_REDEFINE_PROP)
    }

    pub fn fail_read_only(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_READ_ONLY)
    }

    pub fn fail_getter_only(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_GETTER_ONLY)
    }

    pub fn fail_cant_delete(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_DELETE)
    }

    pub fn fail_cant_set_interposed(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_SET_INTERPOSED)
    }

    pub fn fail_cant_define_window_element(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_DEFINE_WINDOW_ELEMENT)
    }

    pub fn fail_cant_delete_window_element(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_DELETE_WINDOW_ELEMENT)
    }

    pub fn fail_cant_define_window_named_property(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_DEFINE_WINDOW_NAMED_PROPERTY)
    }

    pub fn fail_cant_delete_window_named_property(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_DELETE_WINDOW_NAMED_PROPERTY)
    }

    pub fn fail_cant_define_window_non_configurable(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_DEFINE_WINDOW_NC)
    }

    pub fn fail_cant_prevent_extensions(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_PREVENT_EXTENSIONS)
    }

    pub fn fail_cant_set_proto(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_CANT_SET_PROTO)
    }

    pub fn fail_no_named_setter(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_NO_NAMED_SETTER)
    }

    pub fn fail_no_indexed_setter(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_NO_INDEXED_SETTER)
    }

    pub fn fail_not_data_descriptor(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_NOT_DATA_DESCRIPTOR)
    }

    pub fn fail_invalid_descriptor(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_INVALID_DESCRIPTOR)
    }

    pub fn fail_bad_array_length(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_BAD_ARRAY_LENGTH)
    }

    pub fn fail_bad_index(&mut self) -> bool {
        self.fail(JSErrNum::JSMSG_BAD_INDEX)
    }

    pub fn failure_code(&self) -> u32 {
        assert!(!self.ok());
        self.code_ as u32
    }

    #[deprecated]
    #[allow(non_snake_case)]
    pub fn failNoNamedSetter(&mut self) -> bool {
        self.fail_no_named_setter()
    }
}

impl Default for ObjectOpResult {
    fn default() -> ObjectOpResult {
        ObjectOpResult {
            code_: ObjectOpResult_SpecialCodes::Uninitialized as usize,
        }
    }
}

