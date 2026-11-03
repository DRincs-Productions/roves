/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Debug builds only: V8 heap snapshots at chosen explicit collections of a runtime
//! (`JS_GC` with `GCReason::DOM_TESTUTILS`, i.e. Servo's `TestUtils.gc()`), to find what
//! retains objects in a controlled smoke page (`support/v8-smoke/document-retention.html`).
//!
//! Off unless both private environment variables are set, read once per runtime:
//! `ROVES_V8_HEAP_SNAPSHOT_PATH`, an absolute path, and `ROVES_V8_HEAP_SNAPSHOT_GC`, which
//! explicit collections dump (from 1): one number, or up to [`MAX_SNAPSHOTS`] strictly
//! increasing numbers separated by commas (`2,16,32`). One collection writes exactly the given
//! path; a series writes one derived path per collection, `<stem>.gc<N>[.<extension>]`
//! (`scene.heapsnapshot` at 16 becomes `scene.gc16.heapsnapshot`). Files are created new, never
//! overwritten. V8's heap profiler takes and serializes each snapshot (Chrome DevTools'
//! `.heapsnapshot` JSON); it is streamed to its file. One attempt per collection: each target is
//! consumed before its dump, and an invalid configuration or I/O error is reported on stderr,
//! never retried (later targets of a series still run).

use std::cell::Cell;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) const PATH_VAR: &str = "ROVES_V8_HEAP_SNAPSHOT_PATH";
pub(crate) const GC_VAR: &str = "ROVES_V8_HEAP_SNAPSHOT_GC";
/// Bounds a series: each snapshot is a full, potentially large heap dump.
pub(crate) const MAX_SNAPSHOTS: usize = 8;

/// One snapshot: the file it creates and the explicit collection that writes it.
#[derive(Debug)]
pub(crate) struct Target {
    path: PathBuf,
    collection: u32,
}

impl Target {
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

/// Pending snapshot targets, in collection order.
#[derive(Debug)]
pub(crate) struct Request {
    targets: Vec<Target>,
    /// Explicit collections seen while a target is pending.
    seen: Cell<u32>,
    /// The next pending target; `targets.len()` once all are consumed.
    next: Cell<usize>,
}

impl Request {
    /// The request configured by the environment, if any (an invalid one is logged).
    pub(crate) fn from_env() -> Option<Request> {
        parse(std::env::var_os(PATH_VAR), std::env::var_os(GC_VAR)).unwrap_or_else(|error| {
            eprintln!("[roves-heap-snapshot] disabled: {error}");
            None
        })
    }

    /// Counts one explicit collection; the target due at it, once, if any.
    pub(crate) fn take(&self) -> Option<&Target> {
        let target = self.targets.get(self.next.get())?;
        let seen = self.seen.get() + 1;
        self.seen.set(seen);
        if seen != target.collection {
            return None;
        }
        self.next.set(self.next.get() + 1);
        Some(target)
    }
}

/// The request two variable values describe: none when both are unset.
pub(crate) fn parse(path: Option<OsString>, collections: Option<OsString>) -> Result<Option<Request>, String> {
    let (path, collections) = match (path, collections) {
        (None, None) => return Ok(None),
        (Some(path), Some(collections)) => (PathBuf::from(path), collections),
        (Some(_), None) => return Err(format!("{PATH_VAR} is set without {GC_VAR}")),
        (None, Some(_)) => return Err(format!("{GC_VAR} is set without {PATH_VAR}")),
    };
    if !path.is_absolute() {
        return Err(format!("{PATH_VAR} must be an absolute path, not {:?}", path));
    }
    let invalid = || {
        format!(
            "{GC_VAR} must be a positive integer or up to {MAX_SNAPSHOTS} strictly increasing ones \
             separated by commas, not {:?}",
            collections
        )
    };
    let values = collections
        .to_str()
        .ok_or_else(invalid)?
        .split(',')
        .map(|value| value.parse::<u32>().ok().filter(|&value| value > 0))
        .collect::<Option<Vec<u32>>>()
        .filter(|values| values.len() <= MAX_SNAPSHOTS && values.windows(2).all(|pair| pair[0] < pair[1]))
        .ok_or_else(invalid)?;
    let targets = match values[..] {
        [collection] => vec![Target { path, collection }],
        _ => values
            .iter()
            .map(|&collection| {
                derived_path(&path, collection)
                    .map(|path| Target { path, collection })
                    .ok_or_else(|| format!("{PATH_VAR} has no file name to derive a series from: {:?}", path))
            })
            .collect::<Result<_, _>>()?,
    };
    Ok(Some(Request { targets, seen: Cell::new(0), next: Cell::new(0) }))
}

/// `<stem>.gc<collection>[.<extension>]` next to `path`: distinct for distinct collections.
pub(crate) fn derived_path(path: &Path, collection: u32) -> Option<PathBuf> {
    let mut name = path.file_stem()?.to_owned();
    name.push(format!(".gc{collection}"));
    if let Some(extension) = path.extension() {
        name.push(OsStr::new("."));
        name.push(extension);
    }
    Some(path.with_file_name(name))
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

    fn request(path: &Path, collections: &str) -> Result<Option<Request>, String> {
        parse(Some(path.as_os_str().to_owned()), Some(OsString::from(collections)))
    }

    fn targets(request: &Request) -> Vec<(PathBuf, u32)> {
        request.targets.iter().map(|target| (target.path.clone(), target.collection)).collect()
    }

    #[test]
    fn configuration() {
        let path = std::env::temp_dir().join("roves.heapsnapshot");
        assert!(parse(None, None).unwrap().is_none());
        // One collection: exactly the given path (the original single-snapshot contract).
        assert_eq!(vec![(path.clone(), 3)], targets(&request(&path, "3").unwrap().unwrap()));
        for (path, collection) in [
            (Some(path.as_os_str().to_owned()), None),
            (None, Some(OsString::from("1"))),
            (Some(OsString::from("relative.heapsnapshot")), Some(OsString::from("1"))),
            (Some(OsString::new()), Some(OsString::from("1"))),
        ] {
            assert!(parse(path, collection).is_err());
        }
        for collections in [
            "0", "-1", "two", "", " 1", "4294967296", "1,", ",1", "1,,2", "1, 2", "2,2", "16,2", "1,0",
            "1,2,3,4,5,6,7,8,9",
        ] {
            let error = request(&path, collections).unwrap_err();
            assert!(error.contains(GC_VAR), "{collections}: {error}");
        }
        assert_eq!(MAX_SNAPSHOTS, request(&path, "1,2,3,4,5,6,7,8").unwrap().unwrap().targets.len());
    }

    #[test]
    fn series_targets_derive_distinct_paths() {
        let directory = std::env::temp_dir();
        let base = directory.join("document-retention.heapsnapshot");
        assert_eq!(
            vec![
                (directory.join("document-retention.gc2.heapsnapshot"), 2),
                (directory.join("document-retention.gc16.heapsnapshot"), 16),
                (directory.join("document-retention.gc32.heapsnapshot"), 32),
            ],
            targets(&request(&base, "2,16,32").unwrap().unwrap())
        );
        assert_eq!(Some(directory.join("scene.gc7")), derived_path(&directory.join("scene"), 7));
        assert_eq!(Some(directory.join("a.b.gc1.c")), derived_path(&directory.join("a.b.c"), 1));
        // A root has no file name to derive from.
        let root = directory.ancestors().last().unwrap().to_owned();
        assert!(derived_path(&root, 1).is_none());
        assert!(request(&root, "1,2").unwrap_err().contains(PATH_VAR));
    }

    #[test]
    fn each_target_once_at_its_collection() {
        let path = std::env::temp_dir().join("unused.heapsnapshot");
        let first = request(&path, "1").unwrap().unwrap();
        assert_eq!(Some(1), first.take().map(|target| target.collection));
        assert!(first.take().is_none());
        let third = request(&path, "3").unwrap().unwrap();
        let due: [Option<u32>; 5] = std::array::from_fn(|_| third.take().map(|target| target.collection));
        assert_eq!([None, None, Some(3), None, None], due);
        let series = request(&path, "2,4,5").unwrap().unwrap();
        let due: [Option<u32>; 7] = std::array::from_fn(|_| series.take().map(|target| target.collection));
        assert_eq!([None, Some(2), None, Some(4), Some(5), None, None], due);
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
        // A consumed target is not retried after an error.
        let failing = request(&existing, "1").unwrap().unwrap();
        failing.take().unwrap().dump(|_| unreachable!("the file exists"));
        assert!(failing.take().is_none());
        assert_eq!(b"kept", &std::fs::read(&existing).unwrap()[..]);
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

    /// A series in one real runtime: each derived file is written at its own collection, the
    /// base path is never written, and each file sees the heap of its moment.
    #[test]
    fn real_snapshot_series() {
        let directory = scratch("series");
        let base = directory.join("scene.heapsnapshot");
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        // SAFETY: this thread's live runtime.
        let hooks = unsafe { &(*cx).hooks };
        hooks.heap_snapshot.replace(request(&base, "1,3").unwrap());
        let run = |code: &str| {
            unsafe { &*cx }.with_scope(|scope| {
                let source = v8::String::new(scope, code).unwrap();
                v8::Script::compile(scope, source, None).unwrap().run(scope).unwrap();
            })
        };
        run("class RovesSeriesFirst {}; globalThis.first = new RovesSeriesFirst();");
        unsafe { JS_GC(cx, GCReason::DOM_TESTUTILS) };
        run("class RovesSeriesLater {}; globalThis.later = new RovesSeriesLater();");
        unsafe {
            JS_GC(cx, GCReason::DOM_TESTUTILS);
            JS_GC(cx, GCReason::DOM_TESTUTILS);
            JS_GC(cx, GCReason::DOM_TESTUTILS);
        }
        let first = std::fs::read_to_string(directory.join("scene.gc1.heapsnapshot")).unwrap();
        let third = std::fs::read_to_string(directory.join("scene.gc3.heapsnapshot")).unwrap();
        assert!(first.contains("\"RovesSeriesFirst\"") && !first.contains("RovesSeriesLater"));
        assert!(third.contains("\"RovesSeriesFirst\"") && third.contains("\"RovesSeriesLater\""));
        assert!(!base.exists());
        assert!(!directory.join("scene.gc2.heapsnapshot").exists());
        assert_eq!(2, std::fs::read_dir(&directory).unwrap().count());
        drop(runtime);
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// V8 names the cppgc box of a class-based object after its `JSClass`; a nameless class
    /// keeps the generic name, and a borrowed Window wrapper is named apart from its class.
    #[test]
    fn class_boxes_carry_their_class_names() {
        use crate::jsapi::{JS_NewObject, JSClass};
        const fn class(name: *const std::os::raw::c_char) -> JSClass {
            use std::ptr::null;
            JSClass { name, flags: 0, cOps: null(), spec: null(), ext: null(), oOps: null() }
        }
        static ALPHA: JSClass = class(c"RovesSnapshotAlphaClass".as_ptr());
        static BETA: JSClass = class(c"RovesSnapshotBetaClass".as_ptr());
        static NAMELESS: JSClass = class(std::ptr::null());
        static BORROWED: JSClass = class(c"RovesSnapshotBorrowedClass".as_ptr());
        let directory = scratch("names");
        let path = directory.join("names.heapsnapshot");
        let runtime = Runtime::new(JSEngineHandle::for_tests());
        let cx = runtime.raw_cx();
        {
            crate::rooted!(in(cx) let alpha = unsafe { JS_NewObject(cx, &ALPHA) });
            crate::rooted!(in(cx) let beta = unsafe { JS_NewObject(cx, &BETA) });
            crate::rooted!(in(cx) let nameless = unsafe { JS_NewObject(cx, &NAMELESS) });
            crate::rooted!(in(cx) let borrowed = unsafe { JS_NewObject(cx, &BORROWED) });
            for object in [alpha.get(), beta.get(), nameless.get(), borrowed.get()] {
                assert!(!object.is_null());
            }
            let borrowed_box = crate::object::class_box(borrowed.get()).unwrap();
            borrowed_box.window_wrapper.set(true);
            // SAFETY: this thread's live runtime.
            unsafe { &*cx }.with_scope(|scope| write(&path, |sink| scope.take_heap_snapshot(sink))).unwrap();
            borrowed_box.window_wrapper.set(false);
        }
        let snapshot = std::fs::read_to_string(&path).unwrap();
        for name in ["RovesSnapshotAlphaClass", "RovesSnapshotBetaClass", "RovesJsClassBox", "RovesJsWindowWrapper"] {
            assert!(snapshot.contains(&format!("\"{name}\"")), "{name} missing");
        }
        assert!(!snapshot.contains("RovesSnapshotBorrowedClass"));
        drop(runtime);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
