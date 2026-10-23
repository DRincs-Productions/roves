/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Debug builds only: one V8 heap snapshot at the Nth explicit collection of a runtime
//! (`JS_GC` with `GCReason::DOM_TESTUTILS`, i.e. Servo's `TestUtils.gc()`), to find what
//! retains objects in a controlled smoke page (`support/v8-smoke/frame-retention.html`).
//!
//! Off unless both private environment variables are set, read once per runtime:
//! `ROVES_V8_HEAP_SNAPSHOT_PATH`, an absolute path to a new file (never overwritten), and
//! `ROVES_V8_HEAP_SNAPSHOT_GC`, which explicit collection dumps (from 1). V8's heap profiler
//! takes and serializes the snapshot (Chrome DevTools' `.heapsnapshot` JSON); it is streamed
//! to the file. One attempt per runtime: the request is consumed before the dump, and an
//! invalid configuration or I/O error is reported on stderr, never retried.

use std::cell::Cell;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) const PATH_VAR: &str = "ROVES_V8_HEAP_SNAPSHOT_PATH";
pub(crate) const GC_VAR: &str = "ROVES_V8_HEAP_SNAPSHOT_GC";

/// A pending one-shot snapshot request.
#[derive(Debug)]
pub(crate) struct Request {
    path: PathBuf,
    collection: u32,
    /// Explicit collections seen so far; `None` once the request is consumed.
    seen: Cell<Option<u32>>,
}

impl Request {
    /// The request configured by the environment, if any (an invalid one is logged).
    pub(crate) fn from_env() -> Option<Request> {
        parse(std::env::var_os(PATH_VAR), std::env::var_os(GC_VAR)).unwrap_or_else(|error| {
            eprintln!("[roves-heap-snapshot] disabled: {error}");
            None
        })
    }

    /// Counts one explicit collection; true, once, when it is the requested one.
    pub(crate) fn take(&self) -> bool {
        let Some(seen) = self.seen.get() else { return false };
        let seen = seen + 1;
        let due = seen == self.collection;
        self.seen.set(if due { None } else { Some(seen) });
        due
    }

    /// Writes the snapshot `snapshot` produces (see [`write`]) and reports the outcome.
    pub(crate) fn dump(&self, snapshot: impl FnOnce(&mut dyn FnMut(&[u8]) -> bool)) {
        match write(&self.path, snapshot) {
            Ok(bytes) => eprintln!(
                "[roves-heap-snapshot] wrote {bytes} bytes to {} at explicit collection {}",
                self.path.display(),
                self.collection
            ),
            Err(error) => eprintln!(
                "[roves-heap-snapshot] failed at explicit collection {}: {}: {error}",
                self.collection,
                self.path.display()
            ),
        }
    }
}

/// The request two variable values describe: none when both are unset.
pub(crate) fn parse(path: Option<OsString>, collection: Option<OsString>) -> Result<Option<Request>, String> {
    let (path, collection) = match (path, collection) {
        (None, None) => return Ok(None),
        (Some(path), Some(collection)) => (PathBuf::from(path), collection),
        (Some(_), None) => return Err(format!("{PATH_VAR} is set without {GC_VAR}")),
        (None, Some(_)) => return Err(format!("{GC_VAR} is set without {PATH_VAR}")),
    };
    if !path.is_absolute() {
        return Err(format!("{PATH_VAR} must be an absolute path, not {:?}", path));
    }
    let collection = collection
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|&value| value > 0)
        .ok_or_else(|| format!("{GC_VAR} must be a positive integer, not {:?}", collection))?;
    Ok(Some(Request { path, collection, seen: Cell::new(Some(0)) }))
}

/// Creates `path` (never an existing file) and streams into it the chunks `snapshot` passes
/// its sink (V8's `take_heap_snapshot`); returns the bytes written. A failed or empty
/// snapshot removes the file this call created.
pub(crate) fn write(path: &Path, snapshot: impl FnOnce(&mut dyn FnMut(&[u8]) -> bool)) -> std::io::Result<u64> {
    let mut out = BufWriter::new(File::options().write(true).create_new(true).open(path)?);
    let mut written = 0u64;
    let mut failure = None;
    snapshot(&mut |chunk: &[u8]| match out.write_all(chunk) {
        Ok(()) => {
            written += chunk.len() as u64;
            true
        },
        Err(error) => {
            failure = Some(error);
            false
        },
    });
    let result = match failure {
        Some(error) => Err(error),
        None if written == 0 => Err(std::io::Error::other("V8 produced no snapshot")),
        None => out.into_inner().map_err(|error| error.into_error()).and_then(|file| file.sync_all()),
    };
    match result {
        Ok(()) => Ok(written),
        Err(error) => {
            let _ = std::fs::remove_file(path);
            Err(error)
        },
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::*;
    use crate::jsapi::{GCReason, JS_GC};
    use crate::rust::{JSEngineHandle, Runtime};

    /// A fresh directory for one test (no shared state between parallel tests).
    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("roves-heap-snapshot-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn request(path: &Path, collection: &str) -> Result<Option<Request>, String> {
        parse(Some(path.as_os_str().to_owned()), Some(OsString::from(collection)))
    }

    #[test]
    fn configuration() {
        let path = std::env::temp_dir().join("roves.heapsnapshot");
        assert!(parse(None, None).unwrap().is_none());
        assert_eq!(3, request(&path, "3").unwrap().unwrap().collection);
        for (path, collection) in [
            (Some(path.as_os_str().to_owned()), None),
            (None, Some(OsString::from("1"))),
            (Some(OsString::from("relative.heapsnapshot")), Some(OsString::from("1"))),
            (Some(OsString::new()), Some(OsString::from("1"))),
        ] {
            assert!(parse(path, collection).is_err());
        }
        for collection in ["0", "-1", "two", "", " 1", "4294967296"] {
            let error = request(&path, collection).unwrap_err();
            assert!(error.contains(GC_VAR), "{error}");
        }
    }

    #[test]
    fn one_shot_at_the_requested_collection() {
        let path = std::env::temp_dir().join("unused.heapsnapshot");
        let first = request(&path, "1").unwrap().unwrap();
        assert!(first.take());
        assert!(!first.take());
        let third = request(&path, "3").unwrap().unwrap();
        assert_eq!([false, false, true, false, false], std::array::from_fn(|_| third.take()));
    }

    #[test]
    fn io_errors_never_overwrite() {
        let directory = scratch("io");
        let existing = directory.join("existing.heapsnapshot");
        std::fs::write(&existing, b"kept").unwrap();
        let mut called = false;
        let error = write(&existing, |_| called = true).unwrap_err();
        assert_eq!(std::io::ErrorKind::AlreadyExists, error.kind());
        assert!(!called);
        assert_eq!(b"kept", &std::fs::read(&existing).unwrap()[..]);
        let missing = directory.join("missing").join("x.heapsnapshot");
        assert_eq!(std::io::ErrorKind::NotFound, write(&missing, |_| ()).unwrap_err().kind());
        // An empty snapshot (V8 failed to take one) leaves no file behind.
        let empty = directory.join("empty.heapsnapshot");
        assert!(write(&empty, |sink| assert!(sink(&[]))).is_err());
        assert!(!empty.exists());
        // A consumed request is not retried after an error.
        let failing = request(&existing, "1").unwrap().unwrap();
        assert!(failing.take());
        failing.dump(|_| unreachable!("the file exists"));
        assert!(!failing.take());
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Explicit collections of a real runtime: other reasons are not counted, and only the
    /// requested one streams V8's snapshot of the live heap.
    #[test]
    fn real_snapshot_at_the_nth_explicit_collection() {
        let directory = scratch("v8");
        let path = directory.join("frame.heapsnapshot");
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // SAFETY: this thread's live runtime.
        let hooks = unsafe { &(*cx).hooks };
        hooks.heap_snapshot.replace(request(&path, "2").unwrap());
        unsafe { &*cx }.with_scope(|scope| {
            let source = v8::String::new(scope, "class RovesSnapshotEyecatcher {}; globalThis.eyecatcher = new RovesSnapshotEyecatcher();").unwrap();
            v8::Script::compile(scope, source, None).unwrap().run(scope).unwrap();
        });
        unsafe {
            JS_GC(cx, GCReason::API);
            JS_GC(cx, GCReason::DOM_TESTUTILS);
        }
        assert!(!path.exists());
        unsafe { JS_GC(cx, GCReason::DOM_TESTUTILS) };
        let snapshot = std::fs::read_to_string(&path).unwrap();
        assert!(snapshot.starts_with("{\"snapshot\":{\"meta\":"), "{}", &snapshot[..snapshot.len().min(80)]);
        assert!(snapshot.contains("\"RovesSnapshotEyecatcher\""));
        assert!(snapshot.trim_end().ends_with('}'));
        // One shot: later explicit collections write nothing, even once the file is gone.
        std::fs::remove_file(&path).unwrap();
        unsafe { JS_GC(cx, GCReason::DOM_TESTUTILS) };
        assert!(!path.exists());
        drop(runtime);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
