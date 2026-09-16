# Mobile packaging

## Android startup branding

Android displays a native black Roves splash with the desktop icon, Metal Mania
wordmark and animated loading bar. Branding is generated from the engine resources,
separately from bundled game assets and the replaceable launcher icon.
The overlay stays for at least 500 ms and until the initial main document finishes
loading and WebView confirms a drawable visual state. Redirects invalidate stale
completion callbacks; later navigation does not replay the startup splash.
This indicates document readiness, not completion of asynchronous game asset loading.
Android 12+ uses a black, Roves-branded system splash. The app always runs
edge-to-edge: the status and navigation bars are hidden from first frame
(confirmed on a real device), not just during `<video>`/Fullscreen API
content, and reappear only for a temporary edge swipe before auto-hiding
again. Device validation remains pending: cold/warm launch, slow content,
redirects and rotation. Local Java cannot currently launch Gradle.

Desktop continues to use Servo. Android uses the system Android WebView; iOS
uses Apple's WKWebView. Neither mobile container loads Servo, JNI or GStreamer.
Web APIs and rendering therefore follow the installed platform engine.

## Android

Install Java 17 and the Android SDK (platform 37 and build tools 36.0.0), set
ANDROID_HOME, then run from the repository root:

```sh
./mach bundle --android --content-dir /absolute/path/to/dist --output /absolute/path/to/android-output
```

No `mach build --android`, Rust cross compilation or NDK is required for game
packaging. The existing Android app name, orientation, theme color and icon
options are preserved. Release builds use `--android-release` and the existing
APK_SIGNING_KEY_STORE_PATH, APK_SIGNING_KEY_STORE_PASS, APK_SIGNING_KEY_ALIAS and
APK_SIGNING_KEY_PASS environment variables.

For Android Studio or direct Gradle builds, put the game in
`support/android/apk/servoapp/src/main/assets/www`, then run
`./gradlew :servoapp:assembleArm64Debug` inside `support/android/apk`.
The APK is in `servoapp/build/outputs/apk/`. Variant architecture names remain
for compatibility; the WebView app itself has no architecture-specific binaries.

The local origin is `https://appassets.androidplatform.net/` — the bare root,
not `/index.html`: `location.pathname` is `/` at boot, so a client-side
history router's root route matches, the same reasoning as iOS's
`game://content/` below. Loading `/index.html` directly instead was a real,
confirmed bug (a device-tested "Not Found" screen, actually the game's own
router rendering its 404 route because `/index.html` matched nothing) — fixed
2026-09-14, see CUSTOMIZATIONS.md. A path with no matching file falls back to
`index.html` (the same SPA-fallback behavior as iOS and desktop's own
`game://`, see below); only a missing `index.html` itself 404s, with a real
"Not Found" body instead of an empty one. A page's own service worker is
supported too (`ServiceWorkerControllerCompat`, separate from the main
request path) — without it, `navigator.serviceWorker.register()` fails
outright since this origin doesn't resolve on the real network. Storage
persists across app launches. Debug APKs enable WebView debugging; release
APKs do not. Video fullscreen, back navigation and lifecycle are handled by
the container.

A game's own save import/export (`<input type="file">`, `<a download>` on a
Blob/data URL) works out of the box — neither has any default handling in a
plain WebView, so the container wires up `WebChromeClient.onShowFileChooser`
(a real system file picker) and `WebView.setDownloadListener` (blob:/data:
URLs decoded and written natively; a real http(s) URL falls back to the
system Download Manager). Exported files land in
`Documents/<this app's own launcher label>/<fileName>`, a real,
player-visible location (Files app, USB/MTP) via the `MediaStore`
`Documents` collection — no `WRITE_EXTERNAL_STORAGE` permission needed on
API 29+.

## iOS

On macOS, with Xcode and XcodeGen installed, build directly from the repository root:

```sh
./mach bundle --ios --content-dir /absolute/path/to/dist --output /absolute/path/to/ios-output --ios-app-name 'My Game' --ios-bundle-id com.example.mygame
```

No `mach build --ios`, Rust, or any Servo involvement — the same "no compile step" shape as
`--android` above, just Apple's own toolchain (XcodeGen + xcodebuild). Without
`--ios-release`, this produces an unsigned `RovesGame.app` built for the iOS Simulator only
(`CODE_SIGNING_ALLOWED=NO`) — useful to confirm the game stages and builds, not something you
can install on a device or submit anywhere.

`--ios-release` archives and exports a real, signed `.ipa` (Apple's `app-store` export
method) instead, using an actual Apple Distribution identity — set these environment
variables first:

- `IOS_SIGNING_CERTIFICATE_P12_PATH` / `IOS_SIGNING_CERTIFICATE_P12_PASSWORD` — your Apple
  Distribution certificate and private key, exported as a `.p12` (Keychain Access, or
  `openssl pkcs12 -export`, after Apple signs a CSR you generate and submit through
  [developer.apple.com](https://developer.apple.com)'s Certificates page).
- `IOS_SIGNING_PROVISIONING_PROFILE_PATH` — a `.mobileprovision` matching that certificate and
  your app's bundle ID, downloaded from the same Apple Developer Program account.
- `IOS_SIGNING_TEAM_ID` — your Apple Developer Program Team ID.

Without all three, `--ios-release` refuses outright rather than silently falling back to an
unsigned build — the same "fail loudly, no weaker fallback" stance `--android-release` already
takes for `APK_SIGNING_KEY_STORE_PATH` and friends. None of this can be generated by tooling
alone: unlike an Android keystore (self-signed by design), an Apple Distribution certificate
must be countersigned by Apple itself, so the certificate/profile always trace back to a human
with access to the Apple Developer Program account.

For a fully manual, no-`mach` alternative (e.g. to poke around in Xcode directly), staging and
building still work standalone:

```sh
python3 support/ios/bundle.py --content-dir /absolute/path/to/dist --output /absolute/path/to/ios-project --app-name 'My Game' --bundle-id com.example.mygame
cd /absolute/path/to/ios-project && xcodegen generate --spec project.json && open RovesGame.xcodeproj
```

Choose your development team in Xcode, add the required app icons and build for a simulator
or device. The generated project targets iOS 15+ and iPhone/iPad. The output directory must
be new and must not overlap the content directory.

The local origin is `game://content/`, served by a `WKURLSchemeHandler`
(`GameSchemeHandler` in App.swift) instead of a raw `file://` path: relative and
root-relative asset URLs resolve correctly, and `location.pathname` is `/` at
boot, so a client-side history router's root route matches (the same reasoning
as Android's `https://appassets.androidplatform.net/` origin, and desktop's own
`game://content/` protocol handler — see `ports/servoshell/protocols/game.rs`).
A path with no matching file falls back to `index.html`, the same SPA-fallback
behavior as those two. Byte-range requests are honored, so `<video>`/`<audio>`
seeking works. Missing files return 404. `game://` isn't a WebKit secure
context (no Service Workers, some newer APIs gated on that) — an accepted
tradeoff, not an oversight; desktop's own `game://` makes the same call.
A native black splash with the Roves icon, Metal Mania wordmark and animated
bar (`RovesSplashView` in App.swift) covers the initial load, removed once the
WKWebView navigation delegate reports the document finished (with the same
500ms floor as Android's splash) — the iOS equivalent of Android's startup
branding above. The app always runs edge-to-edge: the status bar and, on
Face ID devices, the home indicator are hidden (`GameViewController`'s
`prefersStatusBarHidden`/`prefersHomeIndicatorAutoHidden`) — not yet
confirmed on a physical device or simulator, only reasoned from the API
contract (no macOS/Xcode available where this was written; re-verify before
relying on it). Validate your game's module loading and storage behavior on
the target iOS version before release; device runtime checks (seeking,
splash timing, storage persistence) remain pending, same caveat as Android.

Save export (`<a download>` on a Blob/data URL) works the same way as
Android, adapted to WKWebView: an injected script intercepts the download
click and hands the data to a native bridge, which writes it into this
app's own `Documents/` folder (already private per app on iOS, so — unlike
Android — there's no per-game subfolder). Save **import**
(`<input type="file">`) only works on iOS/iPadOS 18.4+ — confirmed against
Apple's own documentation: WKWebView never supported file input at all on
iOS before that version (unlike macOS, which has had it since 10.12). On an
older iOS a file input stays inert, exactly as before this existed; no
workaround exists below 18.4.

## Compatibility and validation

Servo-specific extensions (including native Roves save APIs) are not
implemented by these containers. Games should use standard web APIs or a
platform bridge provided separately. Packmaster's Android backend and
roves-action's `android`/`ios` inputs already build against this same
WebView-only approach (see those repos' own CLAUDE.md/CUSTOMIZATIONS.md for
current detail) — Packmaster has no iOS packaging (GUI or otherwise) yet,
Android only. The Android workflow no longer produces Servo native-library
downloads.

Python staging and packaging checks can run on Linux. Actual Android compilation
requires a working JDK/SDK and dependency downloads; iOS compilation and runtime
checks require macOS/Xcode. CI runs the real `mach bundle --android` path (not a
hand-rolled Gradle invocation) against smoke-test content, but device runtime
checks are still required for graphics, sound, fullscreen and save persistence.
