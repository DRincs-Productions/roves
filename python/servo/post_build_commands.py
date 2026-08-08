# Copyright 2013 The Servo Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import json
import os
import os.path as path
import shutil
import subprocess
from subprocess import CompletedProcess
from shutil import copy2
from typing import Any, Optional, List, NamedTuple, cast

import mozdebug

from mach.decorators import (
    CommandArgument,
    CommandProvider,
    Command,
)

import servo.util
import servo.platform

from servo.command_base import (
    BuildNotFound,
    CommandBase,
    check_call,
    is_linux,
    is_freebsd,
    is_macosx,
    is_windows,
)
from servo.platform.build_target import is_android
from servo.util import delete

from python.servo.command_base import BuildType

ANDROID_APP_NAME = "org.servo.servoshell"


def _packer_binary_name() -> str:
    return "roves-content-packer.exe" if is_windows() else "roves-content-packer"


class ContentExtraction(NamedTuple):
    """Everything a generated launcher needs to run `roves-content-packer
    extract` for itself before starting the engine — see the `bundle`
    command and CUSTOMIZATIONS.md's content-compression entry. Deliberately
    has no destination directory: every launcher calls `extract` without
    `--dest`, so it picks its own location under the OS temp directory (see
    `support/content-packer/src/extract.rs`) and prints it, which the
    launcher captures — nothing is ever extracted next to the bundle itself.
    """

    #: Where the `.pack` files + `manifest.json` live, relative to wherever
    #: the launcher resolves bundle-relative paths from (e.g. `"dist"`).
    packed_rel_dir: str
    #: Basename of the html file to open once extracted (e.g. `"index.html"`).
    html_basename: str
    #: Absolute path, on the machine running `mach bundle`, to the built
    #: `roves-content-packer` binary — copied into the bundle so the
    #: launcher can run it on the *player's* machine.
    packer_binary: str
    #: Launch args other than the html file itself (`--window-size ...` plus
    #: anything passed after `--` on the `mach bundle` command line).
    extra_args: List[str]


def read_file(filename: str, if_exists: bool = False) -> str | None:
    if if_exists and not path.exists(filename):
        return None
    with open(filename) as f:
        return f.read()


# Copied from Python 3.3+'s shlex.quote()
def shell_quote(arg: str) -> str:
    # use single quotes, and put single quotes into double quotes
    # the string $'b is then quoted as '$'"'"'b'
    return "'" + arg.replace("'", "'\"'\"'") + "'"


def _escape_rust_str(arg: str) -> str:
    """Escape `arg` for embedding as a double-quoted Rust string literal."""
    return arg.replace("\\", "\\\\").replace('"', '\\"')


# Debian's arch names don't match Rust's. Extend as new targets need `--deb`.
_DEBIAN_ARCH_BY_RUST_ARCH = {
    "x86_64": "amd64",
    "aarch64": "arm64",
    "i686": "i386",
}


def _place_bundle_content(
    bundle_root: str,
    html_file: str,
    content_dir: Optional[str],
    compress: str,
    packer_binary: Optional[str],
    compression_level: int,
    max_pack_size: str,
    exclude: List[str],
    boot_include: List[str],
) -> None:
    """Copy `content_dir` (e.g. a built `dist/`) into `bundle_root` at whatever
    relative location `html_file` expects to find it at (e.g. `html_file` of
    `dist/index.html` puts the content at `bundle_root/dist/`).

    When `compress` is `"none"`, this is a plain recursive copy, exactly as
    before. Otherwise `content_dir` is packed into a handful of tar+zstd
    archives instead (see `support/content-packer` and CUSTOMIZATIONS.md) —
    the release then ships those archives, not the individually-browsable
    loose files. `roves-content-packer pack` splits off a small "boot set"
    (the html file itself plus whatever it directly references, or matched by
    `boot_include`) into its own archive(s): the generated launcher extracts
    only *that* eagerly at startup; everything else stays compressed until
    servoshell's own `file:` handler asks for it on demand.
    """
    if not content_dir:
        return
    rel_dir = path.dirname(html_file)
    dest = path.join(bundle_root, rel_dir) if rel_dir else bundle_root
    if compress == "none":
        shutil.copytree(content_dir, dest, dirs_exist_ok=True)
        return

    assert packer_binary is not None
    os.makedirs(dest, exist_ok=True)
    pack_args = [
        packer_binary,
        "pack",
        "--input",
        content_dir,
        "--output",
        dest,
        "--level",
        str(compression_level),
        "--max-pack-size",
        max_pack_size,
        "--html-file",
        path.basename(html_file),
    ]
    for pattern in exclude:
        pack_args += ["--exclude", pattern]
    for pattern in boot_include:
        pack_args += ["--boot-include", pattern]
    subprocess.check_call(pack_args)


@CommandProvider
class PostBuildCommands(CommandBase):
    @Command("run", description="Run Servo", category="post-build")
    @CommandArgument(
        "--android", action="store_true", default=None, help="Run on an Android device through `adb shell`"
    )
    @CommandArgument("--emulator", action="store_true", help="For Android, run in the only emulated device")
    @CommandArgument("--usb", action="store_true", help="For Android, run in the only USB device")
    @CommandArgument(
        "--debugger",
        action="store_true",
        help="Enable the debugger. Not specifying a "
        "--debugger-cmd option will result in the default "
        "debugger being used. The following arguments "
        "have no effect without this.",
    )
    @CommandArgument("--debugger-cmd", default=None, type=str, help="Name of debugger to use.")
    @CommandArgument("--headless", "-z", action="store_true", help="Launch in headless mode")
    @CommandArgument("--software", "-s", action="store_true", help="Launch with software rendering")
    @CommandArgument("params", nargs="...", help="Command-line arguments to be passed through to Servo")
    @CommandBase.common_command_arguments(binary_selection=True)
    @CommandBase.allow_target_configuration
    def run(
        self,
        servo_binary: str,
        params: list[str],
        debugger: bool = False,
        debugger_cmd: str | None = None,
        headless: bool = False,
        software: bool = False,
        emulator: bool = False,
        usb: bool = False,
    ) -> int | None:
        return self._run(servo_binary, params, debugger, debugger_cmd, headless, software, emulator, usb)

    def _run(
        self,
        servo_binary: str,
        params: list[str],
        debugger: bool = False,
        debugger_cmd: str | None = None,
        headless: bool = False,
        software: bool = False,
        emulator: bool = False,
        usb: bool = False,
    ) -> int | None:
        env = self.build_env()
        env["RUST_BACKTRACE"] = "1"
        if software:
            if not (is_linux() or is_freebsd()):
                print("Software rendering is only supported on Linux and FreeBSD at the moment.")
                return

            env["LIBGL_ALWAYS_SOFTWARE"] = "1"
        os.environ.update(env)

        # Make --debugger-cmd imply --debugger
        if debugger_cmd:
            debugger = True

        if is_android(self.target):
            if debugger:
                print("Android on-device debugging is not supported by mach yet. See")
                print("https://github.com/servo/servo/wiki/Building-for-Android#debugging-on-device")
                return
            script = [
                f"am force-stop {ANDROID_APP_NAME}",
            ]
            json_params = shell_quote(json.dumps(params))
            extra = "-e servoargs " + json_params
            rust_log = env.get("RUST_LOG", None)
            if rust_log:
                extra += " -e servolog " + rust_log
            gst_debug = env.get("GST_DEBUG", None)
            if gst_debug:
                extra += " -e gstdebug " + gst_debug
            script += [
                f"am start {extra} {ANDROID_APP_NAME}/{ANDROID_APP_NAME}.MainActivity",
                "sleep 0.5",
                f"echo Servo PID: $(pidof {ANDROID_APP_NAME})",
                f"logcat --pid=$(pidof {ANDROID_APP_NAME})",
                "exit",
            ]
            args = [self.android_adb_path(env)]
            if emulator and usb:
                print("Cannot run in both emulator and USB at the same time.")
                return 1
            if emulator:
                args += ["-e"]
            if usb:
                args += ["-d"]
            shell = subprocess.Popen(args + ["shell"], stdin=subprocess.PIPE)
            shell.communicate(("\n".join(script) + "\n").encode())
            return shell.wait()

        args = [servo_binary]

        if headless:
            args.append("-z")

        # Borrowed and modified from:
        # http://hg.mozilla.org/mozilla-central/file/c9cfa9b91dea/python/mozbuild/mozbuild/mach_commands.py#l883
        if debugger:
            if not debugger_cmd:
                # No debugger name was provided. Look for the default ones on
                # current OS.
                debugger_cmd = mozdebug.get_default_debugger_name(mozdebug.DebuggerSearch.KeepLooking)

            debugger_info = mozdebug.get_debugger_info(debugger_cmd)
            if not debugger_info:
                print("Could not find a suitable debugger in your PATH.")
                return 1

            command = debugger_info.path
            if debugger_cmd == "gdb" or debugger_cmd == "lldb":
                rust_command = "rust-" + debugger_cmd
                try:
                    subprocess.check_call([rust_command, "--version"], env=env, stdout=open(os.devnull, "w"))
                except (OSError, subprocess.CalledProcessError):
                    pass
                else:
                    command = rust_command

            # Prepend the debugger args.
            args = [command] + debugger_info.args + args + params
        else:
            args = args + params

        try:
            check_call(args, env=env)
        except subprocess.CalledProcessError as exception:
            if exception.returncode < 0:
                print(f"Servo was terminated by signal {-exception.returncode}")
            else:
                print(f"Servo exited with non-zero status {exception.returncode}")
            return exception.returncode
        except OSError as exception:
            if exception.errno == 2:
                print("Servo Binary can't be found! Run './mach build' and try again!")
            else:
                raise exception

    @Command(
        "bundle",
        description="Package a completed build into a portable, double-click-runnable bundle",
        category="post-build",
    )
    @CommandArgument(
        "--html-file", default="dist/index.html", help="HTML file (relative to the bundle) to open on launch"
    )
    @CommandArgument("--window-size", default="1280x720", help="Initial window size, e.g. 1280x720")
    @CommandArgument(
        "--output", "-o", default=None, help="Directory to write the bundle to (default: <build dir>/bundle)"
    )
    @CommandArgument(
        "--content-dir",
        default=None,
        help="Directory of web content (e.g. a built dist/) to copy into the bundle, so --html-file "
        "resolves without a separate copy step",
    )
    @CommandArgument(
        "--content-compress",
        default="auto",
        choices=["auto", "none"],
        help="'auto' (default): pack --content-dir into a handful of tar+zstd archives instead of "
        "shipping it as loose, individually browsable files (see support/content-packer and "
        "CUSTOMIZATIONS.md). 'none': copy it in as-is, exactly like before this option existed.",
    )
    @CommandArgument(
        "--content-compression-level",
        default=1,
        type=int,
        help="zstd compression level used by --content-compress=auto. Low values (the default) favor "
        "speed over ratio — see CUSTOMIZATIONS.md for why that's the right tradeoff here.",
    )
    @CommandArgument(
        "--content-max-pack-size",
        default="500M",
        help="Split --content-dir's archives so no single one exceeds this size, e.g. 500M or 1G "
        "(default: 500M). Only meaningful with --content-compress=auto.",
    )
    @CommandArgument(
        "--content-exclude",
        action="append",
        default=[],
        metavar="GLOB",
        help="Glob (relative to --content-dir), repeatable, of files to leave as loose, uncompressed "
        "files instead of packing them — e.g. a save-data or user-config subfolder that shouldn't sit "
        "inside a read-only archive. Only meaningful with --content-compress=auto.",
    )
    @CommandArgument(
        "--content-boot-include",
        action="append",
        default=[],
        metavar="GLOB",
        help="Glob (relative to --content-dir), repeatable, of extra files to add to the eagerly-"
        "extracted 'boot set' beyond the html file and whatever it directly references (e.g. a "
        "splash image shown before the page itself has rendered anything). Only meaningful with "
        "--content-compress=auto.",
    )
    @CommandArgument(
        "--deb",
        action="store_true",
        help="Linux only: build a .deb package instead of the default self-contained play.sh bundle",
    )
    @CommandArgument("--deb-package-name", default="servoshell", help="Package name to use with --deb")
    @CommandArgument("--deb-version", default="0.0.0", help="Package version to use with --deb")
    @CommandArgument("params", nargs="...", help="Extra command-line arguments to pass through to servoshell on launch")
    @CommandBase.common_command_arguments(binary_selection=True)
    def bundle(
        self,
        servo_binary: str,
        html_file: str = "dist/index.html",
        window_size: str = "1280x720",
        output: Optional[str] = None,
        content_dir: Optional[str] = None,
        content_compress: str = "auto",
        content_compression_level: int = 1,
        content_max_pack_size: str = "500M",
        content_exclude: Optional[List[str]] = None,
        content_boot_include: Optional[List[str]] = None,
        deb: bool = False,
        deb_package_name: str = "servoshell",
        deb_version: str = "0.0.0",
        params: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> int | None:
        """Turn a build produced by `./mach build` into something a user can
        double-click to run, instead of the bare build artifact in target/:

        * Windows: a play.exe (built with the "windows" subsystem, so it
          never flashes a console — see ports/servoshell/main.rs for the
          same attribute on servoshell.exe itself) that launches the engine
          binary, which is tucked away in a bin/ subdirectory.
        * macOS: a minimal Roves.app bundle. Finder launches
          Contents/MacOS/<exec> directly, with no Terminal involved at all.
        * Linux: by default, a play.sh next to the engine binary, which
          ships without its executable bit — play.sh is the only supported
          entry point, so a curious `./servoshell-core` fails with
          "Permission denied" instead of launching without the args/
          LD_LIBRARY_PATH it actually needs. With --deb, a proper .deb
          package instead (see _bundle_linux_deb for what it does and does
          not attempt).

        By default (`--content-compress=auto`), `--content-dir` isn't copied
        into the bundle as loose files: it's packed into a handful of
        tar+zstd archives (see support/content-packer and CUSTOMIZATIONS.md),
        and the launcher above extracts them back to plain files at launch
        time instead — so the shipped bundle itself never contains an
        individually browsable copy of the game's web content. Pass
        `--content-compress=none` to get the old, always-loose-files
        behavior back.

        None of this touches target/<profile>/ itself or the binary Cargo
        put there — `./mach run` and friends keep working exactly as before.
        """
        if deb and not is_linux():
            print("--deb is only supported on Linux.")
            return 1

        binary_dir = path.dirname(servo_binary)
        output_dir = output or path.join(binary_dir, "bundle")
        if path.exists(output_dir):
            delete(output_dir)
        os.makedirs(output_dir)

        content_exclude = content_exclude or []
        content_boot_include = content_boot_include or []
        compress_enabled = bool(content_dir) and content_compress != "none"
        packer_binary = self._build_content_packer() if compress_enabled else None

        extra_args = ["--window-size", window_size] + list(params or [])
        # Used only when compression is off — otherwise each launcher builds
        # its own args at *launch* time (see ContentExtraction's docstring).
        launch_args = [html_file] + extra_args
        extraction: Optional[ContentExtraction] = (
            ContentExtraction(
                packed_rel_dir=path.dirname(html_file),
                html_basename=path.basename(html_file),
                packer_binary=cast(str, packer_binary),
                extra_args=extra_args,
            )
            if compress_enabled
            else None
        )

        if is_windows():
            self._bundle_windows(servo_binary, binary_dir, output_dir, launch_args, extraction)
            bundle_root = output_dir
        elif is_macosx():
            bundle_root = path.join(output_dir, "Roves.app", "Contents", "Resources")
            os.makedirs(bundle_root)
            self._bundle_macos(servo_binary, binary_dir, output_dir, launch_args, extraction)
        elif deb:
            self._bundle_linux_deb(
                servo_binary,
                binary_dir,
                output_dir,
                launch_args,
                deb_package_name,
                deb_version,
                html_file,
                content_dir,
                content_compress,
                packer_binary,
                content_compression_level,
                content_max_pack_size,
                content_exclude,
                content_boot_include,
                extraction,
            )
            print(f"Bundle written to {output_dir}")
            return None
        else:
            self._bundle_linux(servo_binary, binary_dir, output_dir, launch_args, extraction)
            bundle_root = output_dir

        _place_bundle_content(
            bundle_root,
            html_file,
            content_dir,
            content_compress,
            packer_binary,
            content_compression_level,
            content_max_pack_size,
            content_exclude,
            content_boot_include,
        )
        print(f"Bundle written to {output_dir}")
        return None

    def _build_content_packer(self) -> str:
        """Builds (release profile) and returns the path to
        `roves-content-packer` — used both here, to `pack` `--content-dir`,
        and copied into the bundle so the generated launcher can run its
        `extract` subcommand at launch time. See `support/content-packer`
        and CUSTOMIZATIONS.md. Always a host-native build (no --target):
        packing happens on the machine running `mach bundle`, and the copy
        shipped into the bundle only ever needs to run on that same host
        platform (Windows/macOS/Linux), never cross-compiled."""
        manifest_path = path.join(self.context.topdir, "support", "content-packer", "Cargo.toml")
        subprocess.check_call(
            ["cargo", "build", "--release", "--manifest-path", manifest_path],
            env=cast(dict[str, str], self.build_env()),
        )
        target_dir = servo.util.get_target_dir()
        return path.join(target_dir, "release", _packer_binary_name())

    def _bundle_windows(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_args: List[str],
        extraction: Optional[ContentExtraction],
    ) -> None:
        bin_dir = path.join(output_dir, "bin")
        os.makedirs(bin_dir)
        shutil.copy(servo_binary, bin_dir)
        for f in os.listdir(binary_dir):
            if f.lower().endswith(".dll"):
                shutil.copy(path.join(binary_dir, f), bin_dir)
                # play.exe (built below) is an MSVC-linked binary too, and
                # lives next to bin/ rather than inside it, so it needs its
                # own copy of the same runtime DLLs alongside itself to
                # start at all.
                shutil.copy(path.join(binary_dir, f), output_dir)

        binary_name = path.basename(servo_binary)

        # When content is packed (see CUSTOMIZATIONS.md's content-compression
        # entry), play.exe first runs `roves-content-packer extract` — with
        # no `--dest`, so it picks its own location under the OS temp
        # directory rather than anything living next to the bundle — and
        # captures the directory it printed, to build the real html-file
        # argument at *this* launch (`--dest` isn't known until then: it's
        # keyed by a hash, computed inside the packer, of this machine's
        # resolved content path). `.output()`, not `.spawn()`: servoshell
        # must not open its html file until extraction actually finishes.
        # `CREATE_NO_WINDOW` keeps that child's console from flashing on
        # screen for an instant before servoshell's own window opens — this
        # is a `windows_subsystem = "windows"` binary, and without that flag
        # spawning any ordinary console-subsystem child (roves-content-packer
        # itself has no reason to be a windows-subsystem binary) briefly pops
        # one open.
        if extraction is not None:
            shutil.copy(extraction.packer_binary, bin_dir)
            packer_name = path.basename(extraction.packer_binary)
            extra_rust_args = ", ".join(
                f'"{_escape_rust_str(a)}".to_string()' for a in extraction.extra_args
            )
            args_snippet = f"""
    let packer = bin_dir.join("{_escape_rust_str(packer_name)}");
    let output = Command::new(&packer)
        .arg("extract")
        .arg("--content-dir")
        .arg(exe_dir.join("{_escape_rust_str(extraction.packed_rel_dir)}"))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .expect("failed to run roves-content-packer extract");
    let cache_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let args: Vec<String> = vec![
        format!("{{}}/{_escape_rust_str(extraction.html_basename)}", cache_dir),
        {extra_rust_args}
    ];
"""
            extra_use = "use std::os::windows::process::CommandExt;\n"
            extra_const = "\nconst CREATE_NO_WINDOW: u32 = 0x0800_0000;\n"
        else:
            rust_args = ", ".join(f'"{_escape_rust_str(a)}".to_string()' for a in launch_args)
            args_snippet = f"""
    let args: Vec<String> = vec![{rust_args}];
"""
            extra_use = ""
            extra_const = ""

        launcher_source = f"""#![windows_subsystem = "windows"]
use std::env;
use std::process::Command;
{extra_use}{extra_const}
fn main() {{
    let exe_dir = env::current_exe()
        .expect("could not resolve own path")
        .parent()
        .expect("exe has no parent directory")
        .to_path_buf();
    let bin_dir = exe_dir.join("bin");
    let servoshell = bin_dir.join("{binary_name}");
{args_snippet}    // Fire-and-forget: servoshell manages its own window from here.
    let _ = Command::new(servoshell)
        .current_dir(&exe_dir)
        .args(&args)
        .spawn();
}}
"""
        launcher_src_path = path.join(output_dir, "_play_launcher.rs")
        with open(launcher_src_path, "w") as f:
            f.write(launcher_source)
        try:
            subprocess.check_call(
                ["rustc", "--edition", "2021", "-O", "-o", path.join(output_dir, "play.exe"), launcher_src_path]
            )
        finally:
            os.remove(launcher_src_path)

    def _bundle_macos(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_args: List[str],
        extraction: Optional[ContentExtraction],
    ) -> None:
        contents_dir = path.join(output_dir, "Roves.app", "Contents")
        macos_dir = path.join(contents_dir, "MacOS")
        os.makedirs(macos_dir)

        core_name = f"{path.basename(servo_binary)}-core"
        core_path = path.join(macos_dir, core_name)
        shutil.copy(servo_binary, core_path)
        os.chmod(core_path, 0o755)
        # servoshell is always linked with `-rpath @executable_path/lib/`
        # (see ports/servoshell/build.rs) — dylibs have to land in a `lib/`
        # subdirectory next to whatever the renamed core binary ends up
        # being, not flat next to it, or dyld won't find them at runtime.
        dylibs = [f for f in os.listdir(binary_dir) if f.endswith(".dylib")]
        if dylibs:
            lib_dir = path.join(macos_dir, "lib")
            os.makedirs(lib_dir)
            for f in dylibs:
                shutil.copy(path.join(binary_dir, f), lib_dir)

        info_plist = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>Roves</string>
    <key>CFBundleIdentifier</key>
    <string>org.servo.servoshell.bundle</string>
    <key>CFBundleName</key>
    <string>Roves</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
</dict>
</plist>
"""
        with open(path.join(contents_dir, "Info.plist"), "w") as f:
            f.write(info_plist)

        # Content (packed by _place_bundle_content) sits in Contents/Resources,
        # a sibling of this script's own directory (Contents/MacOS). `extract`
        # is called with no `--dest`, so it picks (and prints) its own
        # location under the OS temp directory rather than anything inside
        # the .app bundle — captured into $CACHE_DIR and used to build the
        # real html-file argument for this launch.
        if extraction is not None:
            packer_dest = path.join(macos_dir, path.basename(extraction.packer_binary))
            shutil.copy(extraction.packer_binary, packer_dest)
            os.chmod(packer_dest, 0o755)
            quoted_extra = " ".join(shell_quote(a) for a in extraction.extra_args)
            extraction_snippet = (
                f'CACHE_DIR="$("$DIR/{path.basename(extraction.packer_binary)}" extract '
                f'--content-dir "$DIR/../Resources/{extraction.packed_rel_dir}")"\n'
            )
            final_args = f'"$CACHE_DIR/{extraction.html_basename}" {quoted_extra}'
        else:
            extraction_snippet = ""
            final_args = " ".join(shell_quote(a) for a in launch_args)

        launcher_script = f"""#!/usr/bin/env bash
DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
{extraction_snippet}cd "$DIR/../../.."
exec "$DIR/{core_name}" {final_args}
"""
        launcher_path = path.join(macos_dir, "Roves")
        with open(launcher_path, "w") as f:
            f.write(launcher_script)
        os.chmod(launcher_path, 0o755)

    def _bundle_linux(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_args: List[str],
        extraction: Optional[ContentExtraction],
    ) -> None:
        core_name = f"{path.basename(servo_binary)}-core"
        core_path = path.join(output_dir, core_name)
        shutil.copy(servo_binary, core_path)
        for f in os.listdir(binary_dir):
            if ".so" in f:
                shutil.copy(path.join(binary_dir, f), output_dir)
        os.chmod(core_path, 0o644)

        # Content lives right here in output_dir (bundle_root for the
        # non-.deb Linux case), and play.sh already `cd`s into output_dir
        # before running anything, so the packed content is a plain path
        # relative to the script's own directory. `extract` is called with
        # no `--dest`, so it picks (and prints) its own location under the
        # OS temp directory instead — captured into $CACHE_DIR.
        if extraction is not None:
            packer_name = path.basename(extraction.packer_binary)
            packer_dest = path.join(output_dir, packer_name)
            shutil.copy(extraction.packer_binary, packer_dest)
            os.chmod(packer_dest, 0o755)
            quoted_extra = " ".join(shell_quote(a) for a in extraction.extra_args)
            extraction_snippet = (
                f"chmod +x {shell_quote(packer_name)}\n"
                f'CACHE_DIR="$(./{packer_name} extract --content-dir "./{extraction.packed_rel_dir}")"\n'
            )
            final_args = f'"$CACHE_DIR/{extraction.html_basename}" {quoted_extra}'
        else:
            extraction_snippet = ""
            final_args = " ".join(shell_quote(a) for a in launch_args)

        play_sh = f"""#!/usr/bin/env bash
cd "$(dirname "$0")"
chmod +x {shell_quote(core_name)}
export LD_LIBRARY_PATH="$(pwd):$LD_LIBRARY_PATH"
{extraction_snippet}./{core_name} {final_args}
"""
        play_path = path.join(output_dir, "play.sh")
        with open(play_path, "w") as f:
            f.write(play_sh)
        os.chmod(play_path, 0o755)

    def _bundle_linux_deb(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_args: List[str],
        package_name: str,
        version: str,
        html_file: str,
        content_dir: Optional[str],
        content_compress: str,
        packer_binary: Optional[str],
        compression_level: int,
        max_pack_size: str,
        exclude: List[str],
        boot_include: List[str],
        extraction: Optional[ContentExtraction],
    ) -> None:
        """Build a real, installable .deb: `dpkg -i` puts the engine + its
        content under /usr/lib/<package_name>/, a launcher under /usr/bin/,
        and a .desktop entry so it shows up in application launchers. This
        is a functional package, not a lintian-clean one — no changelog,
        no man page, no maintainer scripts; add those if this ever needs to
        go through a real Debian/Ubuntu review.
        """
        if not shutil.which("dpkg-deb"):
            print("--deb requires `dpkg-deb` (from dpkg-dev), which was not found on PATH.")
            raise BuildNotFound("dpkg-deb not found")

        arch = self.target.triple().split("-")[0]
        debian_arch = _DEBIAN_ARCH_BY_RUST_ARCH.get(arch, arch)

        pkg_root = path.join(output_dir, "pkgroot")
        lib_dir = path.join(pkg_root, "usr", "lib", package_name)
        bin_dir = path.join(pkg_root, "usr", "bin")
        applications_dir = path.join(pkg_root, "usr", "share", "applications")
        debian_dir = path.join(pkg_root, "DEBIAN")
        for d in (lib_dir, bin_dir, applications_dir, debian_dir):
            os.makedirs(d)

        core_name = path.basename(servo_binary)
        shutil.copy(servo_binary, path.join(lib_dir, core_name))
        os.chmod(path.join(lib_dir, core_name), 0o755)
        for f in os.listdir(binary_dir):
            if ".so" in f:
                shutil.copy(path.join(binary_dir, f), lib_dir)
        _place_bundle_content(
            lib_dir,
            html_file,
            content_dir,
            content_compress,
            packer_binary,
            compression_level,
            max_pack_size,
            exclude,
            boot_include,
        )

        # /usr/lib/<package_name> is root-owned post-install (dpkg's usual
        # 755) — but that no longer matters here: `extract` is called with no
        # `--dest`, so it picks (and prints) its own writable location under
        # the OS temp directory itself, the same as every other platform
        # above. No cwd-relative or $HOME-relative bookkeeping needed here.
        if extraction is not None:
            packer_name = path.basename(extraction.packer_binary)
            shutil.copy(extraction.packer_binary, path.join(lib_dir, packer_name))
            os.chmod(path.join(lib_dir, packer_name), 0o755)
            quoted_extra = " ".join(shell_quote(a) for a in extraction.extra_args)
            extraction_snippet = (
                f'CACHE_DIR="$(./{packer_name} extract --content-dir "./{extraction.packed_rel_dir}")"\n'
            )
            exec_args = f'"$CACHE_DIR/{extraction.html_basename}" {quoted_extra}'
        else:
            extraction_snippet = ""
            exec_args = " ".join(shell_quote(a) for a in launch_args)

        launcher_script = f"""#!/usr/bin/env bash
cd /usr/lib/{shell_quote(package_name)}
export LD_LIBRARY_PATH="/usr/lib/{shell_quote(package_name)}:$LD_LIBRARY_PATH"
{extraction_snippet}exec ./{shell_quote(core_name)} {exec_args}
"""
        launcher_path = path.join(bin_dir, package_name)
        with open(launcher_path, "w") as f:
            f.write(launcher_script)
        os.chmod(launcher_path, 0o755)

        desktop_entry = f"""[Desktop Entry]
Type=Application
Name={package_name}
Exec=/usr/bin/{package_name}
Terminal=false
Categories=Network;WebBrowser;
"""
        with open(path.join(applications_dir, f"{package_name}.desktop"), "w") as f:
            f.write(desktop_entry)

        installed_size_kb = sum(
            path.getsize(path.join(dirpath, name))
            for dirpath, _dirnames, filenames in os.walk(pkg_root)
            for name in filenames
        ) // 1024
        control = f"""Package: {package_name}
Version: {version}
Section: web
Priority: optional
Architecture: {debian_arch}
Installed-Size: {installed_size_kb}
Maintainer: unspecified <unspecified@example.com>
Description: {package_name} (Servo-based application bundle)
 Packaged by `mach bundle --deb`; see servo/CUSTOMIZATIONS.md for how this
 differs from stock Servo.
"""
        with open(path.join(debian_dir, "control"), "w") as f:
            f.write(control)

        deb_path = path.join(output_dir, f"{package_name}_{version}_{debian_arch}.deb")
        subprocess.check_call(["dpkg-deb", "--build", "--root-owner-group", pkg_root, deb_path])
        delete(pkg_root)

    @Command("coverage-report", description="Create Servo Code Coverage report.", category="post-build")
    @CommandArgument("params", nargs="...", help="Command-line arguments to be passed through to cargo llvm-cov")
    @CommandBase.common_command_arguments(binary_selection=True, build_type=True, coverage_report=True)
    def coverage_report(self, build_type: BuildType, params: Optional[List[str]] = None, **kwargs: Any) -> int:
        target_dir = servo.util.get_target_dir()
        # See `cargo llvm-cov show-env`. We only export the values required at runtime.
        os.environ["CARGO_LLVM_COV"] = "1"
        os.environ["CARGO_LLVM_COV_SHOW_ENV"] = "1"
        os.environ["CARGO_LLVM_COV_TARGET_DIR"] = target_dir
        try:
            cargo_llvm_cov_cmd = ["cargo", "llvm-cov", "report", "--target", self.target.triple()]
            cargo_llvm_cov_cmd.extend(build_type.as_cargo_arg())
            cargo_llvm_cov_cmd.extend(params or [])
            subprocess.check_call(cargo_llvm_cov_cmd)
        except subprocess.CalledProcessError as exception:
            if exception.returncode < 0:
                print(f"`cargo llvm-cov` was terminated by signal {-exception.returncode}")
            else:
                print(f"`cargo llvm-cov` exited with non-zero status {exception.returncode}")
            return exception.returncode
        return 0

    @Command("android-emulator", description="Run the Android emulator", category="post-build")
    @CommandArgument("args", nargs="...", help="Command-line arguments to be passed through to the emulator")
    def android_emulator(self, args: list[str] | None = None) -> int:
        if not args:
            args = []
            print("AVDs created by `./mach bootstrap-android` are servo-arm and servo-x86.")
        emulator = self.android_emulator_path(self.build_env())
        return subprocess.call([emulator] + args)

    @Command("rr-record", description="Run Servo whilst recording execution with rr", category="post-build")
    @CommandArgument("params", nargs="...", help="Command-line arguments to be passed through to Servo")
    @CommandBase.common_command_arguments(binary_selection=True)
    def rr_record(self, servo_binary: str, params: list[str] = []) -> None:
        env = self.build_env()
        env["RUST_BACKTRACE"] = "1"

        servo_cmd = [servo_binary] + params
        rr_cmd = ["rr", "--fatal-errors", "record"]
        try:
            check_call(rr_cmd + servo_cmd)
        except OSError as e:
            if e.errno == 2:
                print("rr binary can't be found!")
            else:
                raise e

    @Command(
        "rr-replay",
        description="Replay the most recent execution of Servo that was recorded with rr",
        category="post-build",
    )
    def rr_replay(self) -> None:
        try:
            check_call(["rr", "--fatal-errors", "replay"])
        except OSError as e:
            if e.errno == 2:
                print("rr binary can't be found!")
            else:
                raise e

    @Command("doc", description="Generate documentation", category="post-build")
    @CommandArgument("params", nargs="...", help="Command-line arguments to be passed through to cargo doc")
    @CommandBase.common_command_arguments(build_configuration=True, build_type=False)
    def doc(self, params: list[str], **kwargs: Any) -> CompletedProcess[bytes] | int | None:
        self.ensure_bootstrapped()

        docs = path.join(servo.util.get_target_dir(), "doc")
        if not path.exists(docs):
            os.makedirs(docs)

        # Document library crates to avoid package name conflict between servoshell
        # and libservo. Besides, main.rs in servoshell is just a stub.
        params.insert(0, "--lib")
        # Documentation build errors shouldn't cause the entire build to fail. This
        # prevents issues with dependencies from breaking our documentation build,
        # with the downside that it hides documentation issues.
        params.insert(0, "--keep-going")

        env = self.build_env()
        env["RUSTC"] = "rustc"
        returncode = self.run_cargo_build_like_command("doc", params, env=env, **kwargs)
        if returncode:
            return returncode

        static = path.join(self.context.topdir, "etc", "doc.servo.org")
        for name in os.listdir(static):
            copy2(path.join(static, name), path.join(docs, name))
