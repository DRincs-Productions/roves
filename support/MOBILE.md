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

## iOS

Stage the game on any host:

```sh
python3 support/ios/bundle.py --content-dir /absolute/path/to/dist --output /absolute/path/to/ios-project --app-name 'My Game' --bundle-id com.example.mygame
```

On macOS, install Xcode and XcodeGen, then inside the generated directory run:

```sh
xcodegen generate --spec project.json
open RovesGame.xcodeproj
```

Choose your development team in Xcode, add the required app icons and build for
a simulator or device. Archive and export using your Apple distribution signing
configuration for an IPA. The generated project targets iOS 15+ and iPhone/iPad.
The output directory must be new and must not overlap the content directory.

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
