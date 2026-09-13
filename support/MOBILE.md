# Mobile packaging

## Android startup branding

Android displays a native black Roves splash with the desktop icon, Metal Mania
wordmark and animated loading bar. Branding is generated from the engine resources,
separately from bundled game assets and the replaceable launcher icon.
The overlay stays for at least 500 ms and until the initial main document finishes
loading and WebView confirms a drawable visual state. Redirects invalidate stale
completion callbacks; later navigation does not replay the startup splash.
This indicates document readiness, not completion of asynchronous game asset loading.
Android 12+ uses a black, Roves-branded system splash.
Device validation remains pending: cold/warm launch, slow content, redirects,
missing index.html, rotation and fullscreen. Local Java cannot currently launch Gradle.

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

The local origin is `https://appassets.androidplatform.net/`: relative and
root-relative asset URLs, JavaScript modules, fetch and browser storage use this
origin. Missing local files return 404 instead of accessing the network.
Storage persists across app launches. Debug APKs enable WebView debugging;
release APKs do not. Video fullscreen, back navigation and lifecycle are handled
by the container.

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

The initial iOS container loads `www/index.html` with WKWebView's file loading
API. Use relative URLs and configure your web bundler accordingly. Root-relative
paths, fetch of local files, service workers and APIs requiring an HTTPS origin
are not supported by this initial file-based container. Validate your game's
module loading and storage behavior on the target iOS version before release.

## Compatibility and validation

Servo-specific extensions (including `game://` and native Roves save APIs) are
not implemented by these containers. Games should use standard web APIs or a
platform bridge provided separately. Android and iOS builds in external projects
such as Packmaster and roves-action will need their own migration: this change
only updates this repository. The Android workflow no longer produces Servo
native-library downloads.

Python staging and packaging checks can run on Linux. Actual Android compilation
requires a working JDK/SDK and dependency downloads; iOS compilation and runtime
checks require macOS/Xcode. CI builds the Android app, but device runtime checks
are still required for graphics, sound, fullscreen and save persistence.
