/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#[cfg(not(any(target_os = "android", target_env = "ohos")))]
use std::path::{Path, PathBuf};

use servo::{ServoUrl, is_reg_domain};

#[cfg(not(any(target_os = "android", target_env = "ohos")))]
pub fn parse_url_or_filename(cwd: &Path, input: &str) -> Result<ServoUrl, ()> {
    // Kiosk/embedded fork: an absolute Windows path (`C:\dir\index.html`) is a
    // perfectly *valid* URL as far as the WHATWG parser is concerned — scheme
    // `c`, opaque path `\dir\index.html` — so `ServoUrl::parse` below returns
    // `Ok` for it and it never reaches the `RelativeUrlWithoutBase` "treat it
    // as a filename" arm. The result gets navigated to as a `c:` URL, which no
    // protocol handler claims, and the window shows "Could not load the
    // requested page: Unsupported scheme". POSIX absolute paths don't hit this
    // (a leading `/` is not a scheme, so they do fail to parse and do reach the
    // filename arm), which is why this only ever bit Windows.
    //
    // It became reachable when the generated `play.exe` launcher started
    // passing an *absolute* html path — the extraction cache directory printed
    // by `roves-content-packer extract`, unknown until launch (see
    // `python/servo/post_build_commands.py` and CUSTOMIZATIONS.md's lazy
    // extraction entry). Before that it passed a bundle-relative path, which
    // parsed as a relative URL and worked.
    //
    // Handled here rather than by hand-assembling a `file:///` string in the
    // launcher: `Url::from_file_path` does the percent-encoding correctly for
    // paths containing spaces, `#`, `?` and friends (Windows temp directories
    // live under the user's profile, so `C:\Users\Mario Rossi\...` is entirely
    // ordinary), and this way *any* Windows path handed to servoshell on the
    // command line works, not just the launcher's.
    if is_windows_absolute_path(input) {
        if let Ok(url) = url::Url::from_file_path(input) {
            return Ok(ServoUrl::from_url(url));
        }
    }

    match ServoUrl::parse(input) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            url::Url::from_file_path(&*cwd.join(input)).map(ServoUrl::from_url)
        },
        Err(_) => Err(()),
    }
}

/// `C:\dir\file.html` / `C:/dir/file.html` (drive-absolute) or
/// `\\server\share\file.html` (UNC). Deliberately not `#[cfg(windows)]`-gated:
/// the conversion above is guarded by `Url::from_file_path` succeeding, which
/// on a non-Windows build rejects both shapes (neither is an absolute path
/// there), so behavior off Windows is unchanged.
#[cfg(not(any(target_os = "android", target_env = "ohos")))]
fn is_windows_absolute_path(input: &str) -> bool {
    let bytes = input.as_bytes();
    let drive_absolute = bytes.len() >= 3 &&
        bytes[0].is_ascii_alphabetic() &&
        bytes[1] == b':' &&
        (bytes[2] == b'\\' || bytes[2] == b'/');
    drive_absolute || input.starts_with(r"\\")
}

#[cfg(not(any(target_os = "android", target_env = "ohos")))]
pub fn get_default_url(
    url_opt: Option<&str>,
    cwd: impl AsRef<Path>,
    exists: impl FnOnce(&PathBuf) -> bool,
    preferences: &crate::prefs::ServoShellPreferences,
) -> ServoUrl {
    // If the url is not provided, we fallback to the homepage in prefs,
    // or a blank page in case the homepage is not set either.
    let mut new_url = None;
    let cmdline_url = url_opt.map(|s| s.to_string()).and_then(|url_string| {
        parse_url_or_filename(cwd.as_ref(), &url_string)
            .inspect_err(|&error| {
                log::warn!("URL parsing failed ({:?}).", error);
            })
            .ok()
    });

    if let Some(url) = cmdline_url.clone() {
        // Check if the URL path corresponds to a file
        match (url.scheme(), url.host(), url.to_file_path()) {
            ("file", None, Ok(ref path)) if exists(path) => {
                new_url = cmdline_url;
            },
            // Kiosk/embedded fork: a bundled launch's positional URL is a
            // `game://content/...` one (see `desktop/protocols/game.rs`'s own doc
            // comment) — no `to_file_path()`/`exists()` check makes sense for it (it
            // isn't a real filesystem path at all), and it's never user-typed input to
            // begin with — always built by this same binary's own `bundle_launch.rs`
            // from a `launch.json` it trusts — so accept it outright, same as the
            // `file:` arm above accepts an already-verified-to-exist path outright.
            ("game", Some(_), _) => {
                new_url = cmdline_url;
            },
            (scheme, None, Err(_)) if is_localhost(scheme) || is_domain_like(scheme) => {
                new_url = ServoUrl::parse(&format!("http://{}:{}", scheme, &url.path())).ok();
            },
            _ => {},
        }
    }

    #[allow(
        clippy::collapsible_if,
        reason = "let chains are not available in 1.85"
    )]
    if new_url.is_none() {
        if let Some(url_opt) = url_opt {
            new_url = location_bar_input_to_url(url_opt, &preferences.searchpage);
        }
    }

    let pref_url = parse_url_or_filename(cwd.as_ref(), &preferences.homepage).ok();
    let blank_url = ServoUrl::parse("about:blank").ok();

    new_url.or(pref_url).or(blank_url).unwrap()
}

/// Interpret an input URL.
///
/// If this is not a valid URL, try to "fix" it by adding a scheme or if all else fails,
/// interpret the string as a search term.
pub(crate) fn location_bar_input_to_url(request: &str, searchpage: &str) -> Option<ServoUrl> {
    let request = request.trim();
    let input_url = ServoUrl::parse(request).ok();
    if let Some(url) = input_url {
        match (url.scheme(), url.host(), url.to_file_path()) {
            (scheme, None, Err(_)) if is_localhost(scheme) || is_domain_like(scheme) => {
                ServoUrl::parse(&format!("http://{}:{}", scheme, &url.path())).ok()
            },
            _ => Some(url),
        }
    } else {
        try_as_file(request)
            .or_else(|| try_as_domain(request))
            .or_else(|| try_as_search_page(request, searchpage))
    }
}

fn try_as_file(request: &str) -> Option<ServoUrl> {
    if request.starts_with('/') {
        return ServoUrl::parse(&format!("file://{}", request)).ok();
    }
    None
}

fn try_as_domain(request: &str) -> Option<ServoUrl> {
    if !request.contains(' ') && is_reg_domain(request) || is_domain_like(request) {
        return ServoUrl::parse(&format!("https://{}", request)).ok();
    }
    None
}

fn try_as_search_page(request: &str, searchpage: &str) -> Option<ServoUrl> {
    if request.is_empty() {
        return None;
    }
    ServoUrl::parse(&searchpage.replace("%s", request)).ok()
}

fn is_domain_like(s: &str) -> bool {
    !s.starts_with('/') && s.contains('/') ||
        (!s.contains(' ') && !s.starts_with('.') && s.split('.').count() > 1)
}

fn is_localhost(s: &str) -> bool {
    s == "localhost"
}
