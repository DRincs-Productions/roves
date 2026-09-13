"""Stage a WKWebView Xcode project; build and sign it on macOS with XcodeGen."""
import argparse
import json
import plistlib
import shutil
from pathlib import Path


def stage(content_dir, output, app_name="Roves Game", bundle_id="org.roves.game"):
    content, output = Path(content_dir).resolve(), Path(output).resolve()
    if not (content / "index.html").is_file():
        raise ValueError("Content directory must contain index.html")
    if output == content or output in content.parents or content in output.parents:
        raise ValueError("Output and content directories must not overlap")
    if output.exists():
        raise ValueError("Output already exists; choose a new directory")
    output.mkdir(parents=True)
    shutil.copy(Path(__file__).with_name("App.swift"), output / "App.swift")
    shutil.copytree(content, output / "www")

    # Native branding shown while the game's own document loads (RovesSplashView, in
    # App.swift) -- kept separate from `www`/the game's own replaceable launcher icon, same
    # split as the Android side of this same mobile pivot (see build.gradle.kts's own
    # `generateRovesBrandAssets` task and its doc comment).
    engine_resources = Path(__file__).resolve().parents[2] / "resources"
    brand_dir = output / "roves-brand"
    brand_dir.mkdir()
    shutil.copy(engine_resources / "servo_1024.png", brand_dir / "servo_1024.png")
    shutil.copy(engine_resources / "fonts" / "MetalMania-Regular.ttf", brand_dir / "MetalMania-Regular.ttf")
    shutil.copy(engine_resources / "fonts" / "MetalMania-OFL.txt", brand_dir / "MetalMania-OFL.txt")
    info = {
        "CFBundleDisplayName": app_name,
        "CFBundleIdentifier": "$(PRODUCT_BUNDLE_IDENTIFIER)",
        "CFBundleExecutable": "$(EXECUTABLE_NAME)",
        "CFBundleName": "$(PRODUCT_NAME)", "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": "1.0", "CFBundleVersion": "1",
        "UILaunchScreen": {},
        "UISupportedInterfaceOrientations": ["UIInterfaceOrientationPortrait", "UIInterfaceOrientationLandscapeLeft", "UIInterfaceOrientationLandscapeRight"],
    }
    with (output / "Info.plist").open("wb") as stream:
        plistlib.dump(info, stream)
    spec = {
        "name": "RovesGame", "options": {"deploymentTarget": {"iOS": "15.0"}},
        "targets": {"RovesGame": {
            "type": "application", "platform": "iOS",
            "sources": [
                {"path": "App.swift"},
                {"path": "www", "type": "folder", "buildPhase": "resources"},
                {"path": "roves-brand", "type": "folder", "buildPhase": "resources"},
            ],
            "settings": {"base": {
                "PRODUCT_BUNDLE_IDENTIFIER": bundle_id, "INFOPLIST_FILE": "Info.plist",
                "SWIFT_VERSION": "5.0", "TARGETED_DEVICE_FAMILY": "1,2",
            }},
        }},
    }
    (output / "project.json").write_text(json.dumps(spec, indent=2) + "\n")
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--content-dir", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--app-name", default="Roves Game")
    parser.add_argument("--bundle-id", default="org.roves.game")
    args = parser.parse_args()
    try:
        result = stage(args.content_dir, args.output, args.app_name, args.bundle_id)
    except ValueError as error:
        parser.error(str(error))
    print(f"Project staged in {result}. On macOS: xcodegen generate --spec project.json")
