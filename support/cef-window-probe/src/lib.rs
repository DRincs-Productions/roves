//! Stage 1 CEF window probe (Windows only). Entry point called by CEF's bootstrap.exe, as in
//! cef-rs' `cefsimple` example; see README.md for the layout, options and acceptance checks.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod server;

/// Called by bootstrap.exe for the browser process and for every CEF subprocess. A null
/// `sandbox_info` means the sandbox is unavailable; the browser process then refuses to start.
#[cfg(windows)]
#[unsafe(no_mangle)]
unsafe extern "C" fn RunWinMain(
    instance: cef::sys::HINSTANCE,
    _command_line: *const u8,
    _command_show: i32,
    sandbox_info: *mut u8,
) -> i32 {
    app::run(cef::MainArgs { instance }, sandbox_info)
}
