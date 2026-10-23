/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `Context::DetachGlobal`, which rusty_v8 152 does not bind (see `incumbent` for why these
//! declare the C++ symbol the prebuilt library exports). With `ContextOptions::global_object`
//! it lets a new context take over an old one's global proxy, as browsers keep one WindowProxy
//! per browsing context across navigations.

unsafe extern "C" {
    #[cfg_attr(not(target_env = "msvc"), link_name = "_ZN2v87Context12DetachGlobalEv")]
    #[cfg_attr(target_env = "msvc", link_name = "?DetachGlobal@Context@v8@@QEAAXXZ")]
    fn context_detach_global(this: *const v8::Context);
}

/// Detaches `context`'s global proxy from its global object, so a new context can reuse it.
/// The old context's code still runs but no longer reaches its global through the proxy.
pub fn detach_global(context: &v8::Context) {
    // SAFETY: a live context; `&Context` is V8's `Context*` (the handle's location).
    unsafe { context_detach_global(context) }
}

#[cfg(test)]
mod tests {
    fn run<'s>(scope: &mut v8::PinScope<'s, '_>, source: &str) -> v8::Local<'s, v8::Value> {
        let source = v8::String::new(scope, source).unwrap();
        v8::Script::compile(scope, source, None).unwrap().run(scope).unwrap()
    }

    #[test]
    fn a_new_context_takes_over_a_detached_global_proxy() {
        crate::initialize_engine();
        let mut isolate = v8::Isolate::new(Default::default());
        v8::scope!(let scope, &mut isolate);
        let template = v8::ObjectTemplate::new(scope);
        let token = v8::String::new(scope, "token").unwrap().into();
        let first = v8::Context::new(scope, v8::ContextOptions { global_template: Some(template), ..Default::default() });
        first.set_security_token(token);
        let observer = v8::Context::new(scope, Default::default());
        observer.set_security_token(token);
        let proxy = first.global(scope);
        {
            let scope = &mut v8::ContextScope::new(scope, first);
            run(scope, "globalThis.page = 'first'");
        }
        {
            let scope = &mut v8::ContextScope::new(scope, observer);
            let key = v8::String::new(scope, "held").unwrap();
            observer.global(scope).set(scope, key.into(), proxy.into());
            assert_eq!(run(scope, "held.page").to_rust_string_lossy(scope), "first");
        }
        super::detach_global(&first);
        let second = v8::Context::new(scope, v8::ContextOptions { global_template: Some(template), global_object: Some(proxy.into()), ..Default::default() });
        second.set_security_token(token);
        assert!(second.global(scope) == proxy, "the same proxy object");
        {
            let scope = &mut v8::ContextScope::new(scope, second);
            assert!(run(scope, "typeof page").to_rust_string_lossy(scope) == "undefined", "a fresh global");
            run(scope, "globalThis.page = 'second'");
        }
        let scope = &mut v8::ContextScope::new(scope, observer);
        assert_eq!(run(scope, "held.page").to_rust_string_lossy(scope), "second", "old references see the new global");
    }
}
