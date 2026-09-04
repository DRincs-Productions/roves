/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The `servoshell` test application.
//!
//! Creates a `Servo` instance with a example implementation of a working
//! web browser.
//!
//! This browser's implementation of `WindowMethods` is built on top
//! of [winit], the cross-platform windowing library.
//!
//! For the engine itself look next door in `components/servo/lib.rs`.
//!
//! [winit]: https://github.com/rust-windowing/winit

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Console;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW;

// Windows only: adds `lib/` (next to this executable) to the process-wide DLL search path,
// used by GStreamer's own plugin loading. A plugin file itself is found via `gst_plugin_
// load_file`'s own `LOAD_WITH_ALTERED_SEARCH_PATH` (relative to that plugin's own
// directory) regardless of this call -- but once found, the OS loader resolves *that
// plugin's own* implicit imports (e.g. `gstnice.dll` needing `nice-10.dll`) through the
// normal, process-wide search order, which by default never includes `lib/` at all. See
// `python/servo/post_build_commands.py`'s `_bundle_windows`/CUSTOMIZATIONS.md's 2026-08-19
// "attempted to shrink the root further, reverted" entry for the full story of finding this
// out the hard way (duplicating every dependency DLL flat next to the binary, rather than
// this) -- that entry's own "why not lower still" section flagged this exact fix as the
// real way to go lower, deliberately deferred at the time. `SetDllDirectoryW` prepends its
// argument to the search order without removing the application directory from it (per its
// own documented behavior), so this is purely additive -- it doesn't change how anything
// already flat next to the binary resolves.
#[cfg(target_os = "windows")]
fn add_lib_dir_to_dll_search_path() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(exe_dir) = exe_path.parent() else {
        return;
    };
    let lib_dir = exe_dir.join("lib");
    if !lib_dir.is_dir() {
        // Not every build has a lib/ subfolder (e.g. a dev build run in-place) -- nothing
        // to add in that case, and SetDllDirectoryW on a nonexistent path would otherwise
        // just silently make that one entry a no-op anyway.
        return;
    }
    let mut wide: Vec<u16> = lib_dir.as_os_str().encode_wide().collect();
    wide.push(0);
    // SAFETY: `wide` is a valid, null-terminated UTF-16 string kept alive for the duration
    // of this call. `SetDllDirectoryW` only reads from it; it doesn't retain the pointer
    // afterward (the OS copies the path internally).
    unsafe {
        SetDllDirectoryW(wide.as_ptr());
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    add_lib_dir_to_dll_search_path();

    #[cfg(target_os = "windows")]
    // SAFETY: No safety related side effects or requirements.
    // See <https://learn.microsoft.com/en-au/windows/console/freeconsole#remarks>
    unsafe {
        // Free the console pop-up when started by double clicking.
        // If started from the command line, nothing would happen.
        let _ = Console::FreeConsole();
        // Try to attach to the console of the parent process.
        // If servo was started from the command line,
        // this would allow continous stdout/stderr output to be seen in the console.
        // Otherwise, the call will fail, which we can ignore.
        let _result = Console::AttachConsole(Console::ATTACH_PARENT_PROCESS);
    }
    cfg_if::cfg_if! {
        if #[cfg(not(any(target_os = "android", target_env = "ohos")))] {
            servoshell::main()
        } else {
            // Android: see ports/servoshell/egl/android/mod.rs.
            // OpenHarmony: see ports/servoshell/egl/ohos/mod.rs.
            println!(
                "Cannot run the servoshell `bin` executable on platforms such as \
                 Android or OpenHarmony. On these platforms you need to compile \
                 the servoshell library as a `cdylib` and integrate it with the \
                 platform app code into an `apk` (android) or `hap` (OpenHarmony).\
                 For Android `mach build` will do these steps automatically for you."
            );
        }
    }
}
