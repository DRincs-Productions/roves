/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::{env, fs, path::PathBuf};
fn main() {
    let source = env::var("TASK_QUEUE_SOURCE")
        .unwrap_or_else(|_| "../../components/script/task_queue.rs".into());
    println!("cargo:rerun-if-env-changed=TASK_QUEUE_SOURCE");
    println!("cargo:rerun-if-changed={source}");
    let source = fs::read_to_string(source).unwrap();
    // Only allocator-accounting derives/attributes and inner module docs need removal.
    // The queue implementation is compiled verbatim; fail if these annotations change.
    assert_eq!(source.matches("#[derive(MallocSizeOf)]").count(), 2);
    let source = source
        .replace("#[derive(MallocSizeOf)]", "")
        .replace("#[ignore_malloc_size_of = \"TaskBox is difficult\"]", "")
        .replace("//!", "//");
    assert!(!source.contains("ignore_malloc_size_of"));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("task_queue.rs"), source).unwrap();
    let path = "../../components/script/task_source.rs";
    println!("cargo:rerun-if-changed={path}");
    let source = fs::read_to_string(path).unwrap();
    let start = source.find("pub(crate) enum TaskSourceName {").unwrap();
    let end = start + source[start..].find("\n}\n").unwrap() + 2;
    fs::write(
        out.join("task_source.rs"),
        format!(
            "#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, strum::VariantArray)]\n{}",
            &source[start..end]
        ),
    )
    .unwrap();
}
