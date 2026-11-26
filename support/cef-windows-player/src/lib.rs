//! Roves CEF player for Windows: CEF accelerated OSR -> wgpu-weld -> wgpu. Entry point
//! called by CEF's bootstrap.exe, which initializes the Windows sandbox broker before loading
//! this DLL; see README.md for options, checks, the opt-in real-game mode and limitations.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod external_link;
#[cfg(windows)]
mod game;
#[cfg(windows)]
mod game_rules;
#[cfg(windows)]
mod icon_rules;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod keys;
#[cfg(windows)]
mod mcp;
#[cfg(windows)]
mod mcp_rules;
#[cfg(windows)]
mod page_init;
#[cfg(windows)]
mod splash;
#[cfg(windows)]
mod stall;
#[cfg(all(windows, feature = "steam"))]
mod steam;
/// `steam.rs` is shared with the macOS and Linux players, which name the host API
/// `crate::host_api`.
#[cfg(all(windows, feature = "steam"))]
use game::host_api;
#[cfg(windows)]
mod timings;
#[cfg(windows)]
mod window_icon;

/// Called by bootstrap.exe for the browser process and every CEF subprocess. The browser
/// process refuses to run without sandbox information (exit 3).
///
/// # Safety
///
/// Called only by CEF's matching bootstrap.exe, which keeps `instance` and `sandbox_info`
/// valid for the duration of the call.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RunWinMain(
    instance: cef::sys::HINSTANCE,
    _command_line: *const u8,
    _command_show: i32,
    sandbox_info: *mut u8,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: forwarded unchanged from bootstrap.exe (see above).
        unsafe { app::run(instance.0.cast(), sandbox_info) }
    }))
    .unwrap_or_else(|_| {
        eprintln!("roves-cef-win panic escaped the browser entry point");
        199
    })
}
