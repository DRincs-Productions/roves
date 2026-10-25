/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(dead_code)]
extern crate self as js;
extern crate self as script_bindings;
extern crate self as servo_base;
pub mod cell {
    pub type DomRefCell<T> = std::cell::RefCell<T>;
}
pub mod id {
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct PipelineId(pub u64);
}
pub mod context {
    pub struct JSContext;
}
pub mod dom {
    pub mod worker {
        pub type TrustedWorkerAddress = ();
    }
}
pub mod script_runtime {
    pub type ScriptThreadEventCategory = ();
}
pub mod task {
    pub trait TaskBox: Send {
        fn name(&self) -> &'static str;
        fn run_box(self: Box<Self>, cx: &mut crate::context::JSContext);
    }
}
mod task_source {
    include!(concat!(env!("OUT_DIR"), "/task_source.rs"));
}
mod task_queue {
    include!(concat!(env!("OUT_DIR"), "/task_queue.rs"));
    #[cfg(test)]
    mod tests {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests.rs"));
    }
}
