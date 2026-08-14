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
import uuid
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
    cd,
    check_call,
    is_linux,
    is_freebsd,
    is_macosx,
    is_windows,
)
from servo.package_commands import check_call_with_randomized_backoff
from servo.platform.build_target import is_android
from servo.util import delete

from python.servo.command_base import BuildType

ANDROID_APP_NAME = "org.servo.servoshell"


def _packer_binary_name() -> str:
    return "roves-content-packer.exe" if is_windows() else "roves-content-packer"


class BundleLaunchInfo(NamedTuple):
    """Everything the shipped bundle's `launch.json` needs, read back at
    startup by the engine's own `ports/servoshell/desktop/bundle_launch.rs`
    to resolve its launch args in-process — see CUSTOMIZATIONS.md's
    single-executable-bundle entry. Always constructed (regardless of
    whether content ended up packed), so `_write_launch_config` has one
    shape to write either way.
    """

    #: Where the `.pack` files + `manifest.json` live, relative to
    #: `launch.json`'s own directory (e.g. `"dist"`) — `None` when content
    #: isn't packed (`--content-compress=none`, or no `--content-dir` at
    #: all), in which case `html_file` below is used directly instead.
    packed_rel_dir: Optional[str]
    #: Bundle-relative path (relative to `launch.json`'s own directory) to
    #: the html file to open — only used when `packed_rel_dir` is `None`,
    #: since a packed build's actual entry file is only known after
    #: extraction (recorded in `manifest.json`'s `entry_html`, read by the
    #: engine itself, not threaded through here).
    html_file: str
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


# Debian's arch names don't match Rust's. Extend as new targets need `--deb`.
_DEBIAN_ARCH_BY_RUST_ARCH = {
    "x86_64": "amd64",
    "aarch64": "arm64",
    "i686": "i386",
}


def _sanitize_msi_version(version: str) -> str:
    """WiX's `Product/@Version` must be a dotted run of 1-4 integers, each
    0-65535 — an MSI file-format constraint, not a WiX one — unlike
    `--package-version` for `--deb`, which lands in a free-text Debian
    control field and accepts anything. Strips a leading `v` (a common git
    tag convention, e.g. `v1.2.3`) and raises rather than silently coercing
    an incompatible value: mangling a version string a game's own CI might
    rely on for update checks is worse than failing loudly at bundle time.
    """
    trimmed = version[1:] if version[:1].lower() == "v" and version[1:2].isdigit() else version
    parts = trimmed.split(".")
    if not (1 <= len(parts) <= 4) or not all(p.isdigit() and int(p) <= 65535 for p in parts):
        raise ValueError(
            f"--package-version {version!r} isn't a valid MSI version (1-4 dot-separated "
            "integers, each 0-65535, e.g. 1.2.3) — required for --msi."
        )
    return trimmed


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
    game_name: Optional[str],
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

    `game_name` (the same value already resolved for the window title, see
    `_resolve_window_title`) is written into the packed manifest via `--name`
    so the engine's extraction cache directory is named after the game
    instead of a bare content hash — see `Manifest::name`/`extract::
    default_dest` and CUSTOMIZATIONS.md.
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
    if game_name:
        pack_args += ["--name", game_name]
    subprocess.check_call(pack_args)


def _resolve_window_title(content_dir: str) -> Optional[str]:
    """Reads the game's own display name — `manifest.json`'s `name` (a standard web
    app manifest field, and the file actually ships inside `content_dir`, e.g. `dist/`,
    since a bundler copies `public/` files there verbatim), or, failing that,
    `package.json`'s `name` one directory up from `content_dir` (the common
    Vite/webpack convention: `package.json` lives next to the project, `content_dir`
    is its built `dist/` one level below — a source file, so only available here at
    bundle time, never inside the shipped `content_dir` itself). Used verbatim as the
    window title, exactly as written — this doesn't prepend "Roves" or anything else;
    that's the content author's own call to make in `name` (see e.g. `test-page/public/
    manifest.json`'s `"name": "Roves test-page"`). Returns `None` if neither file
    exists or has a usable `name`, leaving the window title exactly as it was before
    this feature (the page's own `document.title`) — see CUSTOMIZATIONS.md.
    """
    for candidate in (
        path.join(content_dir, "manifest.json"),
        path.join(content_dir, "..", "package.json"),
    ):
        if not path.exists(candidate):
            continue
        try:
            with open(candidate) as f:
                name = json.load(f).get("name")
        except (json.JSONDecodeError, OSError):
            continue
        if name:
            return str(name)
    return None


def _write_launch_config(config_dir: str, info: BundleLaunchInfo) -> None:
    """Writes `launch.json` into `config_dir` (always a sibling of the real,
    single shipped binary — see the `_bundle_*` methods below). This is the
    entire replacement for what used to be a separate generated launcher
    executable: previously each platform got its own generated source/script
    hardcoding these same values and (for packed content) shelling out to
    `roves-content-packer extract` at startup; now the engine reads this
    plain data file and calls the equivalent extraction code itself,
    in-process, as a library — see `ports/servoshell/desktop/bundle_launch.rs`.
    """
    config = {
        "content_dir": info.packed_rel_dir,
        "url": None if info.packed_rel_dir else info.html_file,
        "args": info.extra_args,
    }
    with open(path.join(config_dir, "launch.json"), "w") as f:
        json.dump(config, f)


# See `_write_diagnostic_script` below for why these exist. Kept as plain,
# readable script text (not templated/generated) so anyone can read exactly
# what runs without needing to trace through this file — the whole point is
# to be inspectable by a non-technical tester asked to run it.
_DIAGNOSE_BAT = """@echo off
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo ===================================================
echo  Roves startup diagnostic
echo ===================================================
echo.
echo Launching play.exe ...
echo (Close the game window normally when you're done, or press Ctrl+C here
echo  to stop early.)
echo.

play.exe
set EXITCODE=%ERRORLEVEL%

echo.
echo ===================================================
echo  play.exe exited with code %EXITCODE%
if %EXITCODE% NEQ 0 (
    echo  ** This looks like a launch failure. **
)
echo ===================================================
echo.
echo Looking for the most recent roves.log under %LOCALAPPDATA% ...
echo.

set "FOUND="
for /f "delims=" %%F in ('dir /b /s /o-d "%LOCALAPPDATA%\\roves.log" 2^>nul') do (
    if not defined FOUND set "FOUND=%%F"
)

if defined FOUND (
    echo Found: !FOUND!
    echo.
    echo --- log contents ---
    type "!FOUND!"
    echo ---------------------
) else (
    echo No roves.log found. If play.exe crashed before logging even
    echo started, that itself is useful information to report.
)

echo.
echo Copy everything above into a bug report for this game.
echo.
pause
"""

_DIAGNOSE_SH = """#!/bin/sh
cd "$(dirname "$0")"

echo "==================================================="
echo " Roves startup diagnostic"
echo "==================================================="
echo

if [ -x "./Roves.app/Contents/MacOS/Roves" ]; then
    BIN="./Roves.app/Contents/MacOS/Roves"
elif [ -x "./play" ]; then
    BIN="./play"
else
    echo "Could not find the game binary next to this script."
    exit 1
fi

echo "Launching $BIN ..."
echo "(Close the game window normally when you're done, or press Ctrl+C"
echo " here to stop early.)"
echo

"$BIN"
EXIT_CODE=$?

echo
echo "==================================================="
echo " $BIN exited with code $EXIT_CODE"
if [ "$EXIT_CODE" -ne 0 ]; then
    echo " ** This looks like a launch failure. **"
fi
echo "==================================================="
echo

if [ "$(uname)" = "Darwin" ]; then
    CACHE_ROOT="$HOME/Library/Caches"
else
    CACHE_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}"
fi

echo "Looking for the most recent roves.log under $CACHE_ROOT ..."
echo
LOG=$(find "$CACHE_ROOT" -iname roves.log 2>/dev/null -exec ls -t {} + 2>/dev/null | head -1)

if [ -n "$LOG" ]; then
    echo "Found: $LOG"
    echo
    echo "--- log contents ---"
    cat "$LOG"
    echo "---------------------"
else
    echo "No roves.log found. If the game crashed before logging even"
    echo "started, that itself is useful information to report."
fi

echo
echo "Copy everything above into a bug report for this game."
echo
"""


def _write_diagnostic_script(dest_dir: str) -> None:
    """Writes a small, standalone launch-and-report script next to the
    bundle's executable — `diagnose.bat` on Windows, `diagnose.sh` on
    macOS/Linux — gated behind `bundle`'s own `--diagnostic-script` (off by
    default: a real game's shipped release has no reason to carry
    engine-internal debug tooling players never asked for; CI's own
    `.github/workflows/test.yml` always passes it).

    Exists for exactly the failure mode CUSTOMIZATIONS.md's `launch.json`
    entries describe: a Windows build has no console
    (`#![windows_subsystem = "windows"]`), so a launch that dies before a
    window ever appears looks like literally nothing happened, with
    nothing on-screen to report back. Running this script instead of the
    game directly launches the exact same binary from a console/terminal
    that stays open afterward, printing the exit code and `roves.log`'s
    contents inline — something a non-technical tester can copy-paste into
    a bug report without knowing what a log file even is, let alone where
    to find one.

    Placed alongside `mach bundle`'s shared "stage" output (see `bundle`'s
    own `stage_dir`), not inside `_place_bundle_content`'s `bundle_root` —
    on macOS that distinction matters: `bundle_root` there is
    `Roves.app/Contents/Resources`, invisible to a user browsing the
    bundle folder, whereas `stage_dir` is the directory `Roves.app` itself
    sits in. Because `--msi`/`--dmg` wrap this same `stage_dir` into their
    installer, the script ships inside those too — not just the portable
    output. Deliberately not written for `--deb`: a `.deb` install runs
    from `/usr/bin` via a normal terminal already showing stdout/stderr
    directly, so this script would have nothing to add there.
    """
    if is_windows():
        script_path = path.join(dest_dir, "diagnose.bat")
        script = _DIAGNOSE_BAT
        newline = "\r\n"
    else:
        script_path = path.join(dest_dir, "diagnose.sh")
        script = _DIAGNOSE_SH
        newline = "\n"
    with open(script_path, "w", newline=newline) as f:
        f.write(script)
    if not is_windows():
        os.chmod(script_path, 0o755)


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
    @CommandArgument(
        "--msi",
        action="store_true",
        help="Windows only: build an installable .msi package instead of the default self-contained "
        "play.exe bundle",
    )
    @CommandArgument(
        "--dmg",
        action="store_true",
        help="macOS only: wrap the default Roves.app bundle in an installable .dmg disk image instead "
        "of shipping the .app on its own",
    )
    @CommandArgument("--package-name", default="roves", help="Package name to use with --deb/--msi/--dmg")
    @CommandArgument("--package-version", default="0.0.0", help="Package version to use with --deb/--msi/--dmg")
    @CommandArgument(
        "--diagnostic-script",
        action="store_true",
        help="Ship a diagnose.bat/diagnose.sh alongside the bundle (portable/--msi/--dmg only, not --deb) "
        "that launches the game from a console and prints its exit code plus roves.log inline — for "
        "testers to run when a build appears to do nothing. Off by default: a real release has no "
        "reason to carry this debug tooling unasked.",
    )
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
        msi: bool = False,
        dmg: bool = False,
        package_name: str = "roves",
        package_version: str = "0.0.0",
        diagnostic_script: bool = False,
        params: Optional[List[str]] = None,
        **kwargs: Any,
    ) -> int | None:
        """Turn a build produced by `./mach build` into something a user can
        double-click to run, instead of the bare build artifact in target/:

        * Windows: a single play.exe (built with the "windows" subsystem, so
          it never flashes a console — see ports/servoshell/main.rs for the
          same attribute the underlying engine binary already carries). With
          --msi, an installable .msi package instead (see _wrap_windows_msi).
        * macOS: a minimal Roves.app bundle whose Contents/MacOS/Roves *is*
          the engine binary itself. Finder launches it directly, no
          Terminal involved at all. With --dmg, that same .app wrapped in an
          installable .dmg disk image instead (see _wrap_macos_dmg).
        * Linux: by default, a single `play` binary. With --deb, a proper
          .deb package instead (see _bundle_linux_deb for what it does and
          does not attempt).

        Every platform's *portable* output ships exactly one executable — no
        separate launcher process and no `roves-content-packer` binary
        alongside it. A small `launch.json` sits next to the binary instead,
        read back by the engine's own `ports/servoshell/desktop/
        bundle_launch.rs` at startup to resolve its launch args (window
        size, content location) and, for packed content, run the equivalent
        of `roves-content-packer extract` in-process as a library call
        rather than a separate program. An installer (--deb/--msi/--dmg)
        wraps that exact same portable output — the portable staging files
        themselves aren't left behind once the installer is built.

        By default (`--content-compress=auto`), `--content-dir` isn't copied
        into the bundle as loose files: it's packed into a handful of
        tar+zstd archives (see support/content-packer and CUSTOMIZATIONS.md),
        extracted back to plain files in-process at launch time instead — so
        the shipped bundle itself never contains an individually browsable
        copy of the game's web content. Pass `--content-compress=none` to
        get the old, always-loose-files behavior back.

        None of this touches target/<profile>/ itself or the binary Cargo
        put there — `./mach run` and friends keep working exactly as before.
        """
        if deb and not is_linux():
            print("--deb is only supported on Linux.")
            return 1
        if msi and not is_windows():
            print("--msi is only supported on Windows.")
            return 1
        if dmg and not is_macosx():
            print("--dmg is only supported on macOS.")
            return 1

        binary_dir = path.dirname(servo_binary)
        # abspath, not the possibly-relative `output` as-is: `_wrap_windows_msi` below
        # `cd`s into a separate build directory before invoking candle/light, and WiX
        # resolves each <File Source="..."> path (baked into the .wxs from `stage_dir`,
        # itself derived from `output_dir`) relative to *its own* cwd at that point — a
        # relative `output_dir` resolves to the wrong place once the cwd changes out from
        # under it (see CUSTOMIZATIONS.md's --msi/--dmg installer entry, "Correction").
        output_dir = path.abspath(output or path.join(binary_dir, "bundle"))
        if path.exists(output_dir):
            delete(output_dir)
        os.makedirs(output_dir)

        content_exclude = content_exclude or []
        content_boot_include = content_boot_include or []
        compress_enabled = bool(content_dir) and content_compress != "none"
        packer_binary = self._build_content_packer() if compress_enabled else None

        # `params` (this command's `nargs="..."` catch-all — see its own
        # `@CommandArgument` above) is meant for flags the *shipped game*
        # should see at launch (forwarded verbatim into `launch.json`'s
        # "args", read back by `bundle_launch.rs`). It is NOT meant for this
        # `mach bundle` invocation's own flags — but if one of those (e.g.
        # `--package-name`/`--package-version`, meant only for naming a
        # --deb/--msi/--dmg output) ever ends up here anyway — seen in
        # practice, cause not fully understood, filed as a real incident —
        # blindly forwarding it into launch.json silently breaks every
        # future launch of the resulting bundle: servoshell's own CLI
        # parser rejects the unrecognized flag and exits immediately,
        # before any window ever appears or anything gets logged (see
        # CUSTOMIZATIONS.md's startup-file-logging entry, which is what
        # actually surfaced this). Guard against that class of bug
        # unconditionally, regardless of how a reserved flag got into
        # `params` in the first place. `_BOOLEAN_ARGPARSE_ACTIONS` mirrors
        # which of this command's own flags take no value (so skipping a
        # leaked flag doesn't also eat the next, unrelated, legitimate
        # passthrough token).
        _BOOLEAN_ARGPARSE_ACTIONS = {"store_true", "store_false", "store_const", "count"}
        takes_value_by_flag = {}
        for flag_names, flag_kwargs in cast(List[Any], self.__class__.bundle._mach_command.arguments):
            for flag_name in flag_names:
                takes_value_by_flag[flag_name] = flag_kwargs.get("action") not in _BOOLEAN_ARGPARSE_ACTIONS

        extra_args = ["--window-size", window_size]
        skip_next_as_value = False
        leaked_reserved_flags = []
        for p in params or []:
            if skip_next_as_value:
                skip_next_as_value = False
                continue
            if p in takes_value_by_flag:
                leaked_reserved_flags.append(p)
                skip_next_as_value = takes_value_by_flag[p]
                continue
            extra_args.append(p)
        if leaked_reserved_flags:
            print(
                f"warning: mach bundle's own flag(s) {leaked_reserved_flags} ended up in the "
                "passthrough args meant for the shipped game (see post_build_commands.py's "
                "`bundle` for why this is dangerous) — dropping them, along with the value "
                "each one takes, rather than writing a broken launch.json."
            )
        window_title = _resolve_window_title(content_dir) if content_dir else None
        if window_title:
            extra_args += ["--window-title", window_title]
        launch_info = BundleLaunchInfo(
            packed_rel_dir=path.dirname(html_file) if compress_enabled else None,
            html_file=html_file,
            extra_args=extra_args,
        )

        # --msi/--dmg wrap the exact same portable output the plain windows/macOS
        # branches below produce — built into a throwaway staging directory first,
        # so `_bundle_windows`/`_bundle_macos` and `_place_bundle_content` don't need
        # to know or care whether their output ends up shipped as-is or wrapped into
        # an installer afterward.
        stage_dir = path.join(output_dir, "_stage") if (msi or dmg) else output_dir

        if is_windows():
            if msi:
                os.makedirs(stage_dir)
            self._bundle_windows(servo_binary, binary_dir, stage_dir, launch_info)
            bundle_root = stage_dir
        elif is_macosx():
            if dmg:
                os.makedirs(stage_dir)
            bundle_root = path.join(stage_dir, "Roves.app", "Contents", "Resources")
            os.makedirs(bundle_root)
            self._bundle_macos(servo_binary, binary_dir, stage_dir, launch_info)
        elif deb:
            self._bundle_linux_deb(
                servo_binary,
                binary_dir,
                output_dir,
                launch_info,
                package_name,
                package_version,
                html_file,
                content_dir,
                content_compress,
                packer_binary,
                content_compression_level,
                content_max_pack_size,
                content_exclude,
                content_boot_include,
                window_title,
            )
            print(f"Bundle written to {output_dir}")
            return None
        else:
            self._bundle_linux(servo_binary, binary_dir, output_dir, launch_info)
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
            window_title,
        )

        if diagnostic_script:
            # `stage_dir`, not `bundle_root`: on macOS `bundle_root` is
            # `Roves.app/Contents/Resources`, invisible to a user browsing
            # the bundle — `stage_dir` is where `Roves.app` itself (or
            # play.exe/play) sits. See `_write_diagnostic_script`.
            _write_diagnostic_script(stage_dir)

        if msi:
            self._wrap_windows_msi(stage_dir, output_dir, package_name, package_version)
            delete(stage_dir)
        elif dmg:
            self._wrap_macos_dmg(stage_dir, output_dir, package_name, package_version)
            delete(stage_dir)

        print(f"Bundle written to {output_dir}")
        return None

    def _build_content_packer(self) -> str:
        """Builds (release profile) and returns the path to
        `roves-content-packer` — used to `pack` `--content-dir` here on the
        machine running `mach bundle`. Not shipped into the bundle itself:
        extraction on the player's machine happens in-process, inside the
        engine binary, which already links `roves_content_packer` as a
        library (see `ports/servoshell/desktop/bundle_launch.rs` and
        CUSTOMIZATIONS.md). Always a host-native build (no --target) —
        packing only ever needs to run here, on this host."""
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
        launch_info: BundleLaunchInfo,
    ) -> None:
        """The engine binary itself, copied and renamed to `play.exe`,
        sitting flat in `output_dir` — no `bin/` subdirectory, no separate
        launcher. `#![windows_subsystem = "windows"]` (so it never flashes a
        console) is already set on this binary itself, in
        `ports/servoshell/main.rs`. See `_write_launch_config`/
        `ports/servoshell/desktop/bundle_launch.rs` for how it resolves its
        launch args and, for packed content, extracts it, entirely
        in-process — no `roves-content-packer.exe` is shipped here either.
        """
        shutil.copy(servo_binary, path.join(output_dir, "play.exe"))
        for f in os.listdir(binary_dir):
            if f.lower().endswith(".dll"):
                shutil.copy(path.join(binary_dir, f), output_dir)
        _write_launch_config(output_dir, launch_info)

    def _wrap_windows_msi(self, stage_dir: str, output_dir: str, package_name: str, version: str) -> None:
        """Wraps the portable Windows bundle already built into `stage_dir`
        (by `_bundle_windows` + `_place_bundle_content`) into an installable
        `.msi`, via WiX's `candle`/`light` — the same toolset upstream's own
        `./mach package` uses for stock Servo (see package_commands.py and
        support/windows/servoshell.wxs.mako), adapted here as a generic
        recursive harvest of `stage_dir` instead of that template's
        hardcoded servoshell.exe + resources/ layout, since a bundle's
        actual contents (DLL set, whether/where packed content ended up)
        vary per game and per --content-compress setting.
        """
        for tool in ("candle", "light"):
            if not shutil.which(tool):
                print(f"--msi requires the WiX Toolset ('{tool}' not found on PATH).")
                raise BuildNotFound(f"{tool} not found")

        msi_version = _sanitize_msi_version(version)
        # Deterministic per-package-name, not per-build: WiX's MajorUpgrade
        # mechanism (below, in the template) uses a stable UpgradeCode to
        # recognize "this is a newer version of the same product" across
        # separate `mach bundle` invocations — a fresh random one every
        # build would make every install look like an unrelated product.
        upgrade_code = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"roves.bundle.{package_name}"))

        import mako.template

        msi_build_dir = path.join(path.dirname(stage_dir), "_msi-build")
        if path.exists(msi_build_dir):
            delete(msi_build_dir)
        os.makedirs(msi_build_dir)

        template_path = path.join(self.context.topdir, "support", "windows", "roves-bundle.wxs.mako")
        template = mako.template.Template(open(template_path).read())
        wxs_path = path.join(msi_build_dir, "Bundle.wxs")
        with open(wxs_path, "w") as f:
            f.write(
                template.render(
                    package_name=package_name,
                    upgrade_code=upgrade_code,
                    msi_version=msi_version,
                    stage_dir=stage_dir,
                )
            )

        try:
            with cd(msi_build_dir):
                subprocess.check_call(["candle", "Bundle.wxs"])
                subprocess.check_call(["light", "Bundle.wixobj"])
        except subprocess.CalledProcessError as e:
            print("WiX candle/light exited with return value %d" % e.returncode)
            raise

        msi_path = path.join(output_dir, f"{package_name}_{version}.msi")
        if path.exists(msi_path):
            os.remove(msi_path)
        shutil.move(path.join(msi_build_dir, "Bundle.msi"), msi_path)
        delete(msi_build_dir)
        print(f"Packaged into {msi_path}")

    def _bundle_macos(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_info: BundleLaunchInfo,
    ) -> None:
        """The engine binary itself becomes `Contents/MacOS/Roves` directly
        — no wrapper script. Finder (and `CFBundleExecutable` below) launch
        it as-is."""
        contents_dir = path.join(output_dir, "Roves.app", "Contents")
        macos_dir = path.join(contents_dir, "MacOS")
        os.makedirs(macos_dir)

        exe_path = path.join(macos_dir, "Roves")
        shutil.copy(servo_binary, exe_path)
        os.chmod(exe_path, 0o755)
        # Linked with `-rpath @executable_path/lib/` (see
        # ports/servoshell/build.rs) — dylibs have to land in a `lib/`
        # subdirectory next to the binary, not flat next to it, or dyld
        # won't find them at runtime.
        dylibs = [f for f in os.listdir(binary_dir) if f.endswith(".dylib")]
        if dylibs:
            lib_dir = path.join(macos_dir, "lib")
            os.makedirs(lib_dir)
            for f in dylibs:
                shutil.copy(path.join(binary_dir, f), lib_dir)

        # CFBundleIdentifier deliberately still says "servoshell", not
        # "roves" — the 2026-08-07 rename entry in CUSTOMIZATIONS.md already
        # considered and deliberately deferred this exact string, since a
        # bundle identifier (unlike a display label) affects macOS-level
        # identity (defaults/prefs, TCC permission grants, code signing) and
        # needs its own explicit decision, not a mechanical rename.
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
        # a sibling of Contents/MacOS — `bundle_launch.rs` knows to look
        # there specifically on macOS.
        _write_launch_config(macos_dir, launch_info)

    def _wrap_macos_dmg(self, stage_dir: str, output_dir: str, package_name: str, version: str) -> None:
        """Wraps the Roves.app bundle already built into `stage_dir` (by
        `_bundle_macos` + `_place_bundle_content`) into an installable
        `.dmg` disk image via `hdiutil` — the same tool upstream's own
        `./mach package` uses for stock Servo's `Servo.app` (see
        package_commands.py's `package` command), adapted here to wrap our
        content-bearing `Roves.app` instead. An `/Applications` symlink
        sits alongside the `.app` inside the mounted volume so Finder's
        usual drag-to-install gesture works.
        """
        if not shutil.which("hdiutil"):
            print("--dmg requires `hdiutil`, which was not found on PATH.")
            raise BuildNotFound("hdiutil not found")

        os.symlink("/Applications", path.join(stage_dir, "Applications"))
        dmg_path = path.join(output_dir, f"{package_name}-{version}.dmg")
        if path.exists(dmg_path):
            os.remove(dmg_path)
        try:
            # hdiutil gives "Resource busy" failures on GitHub Actions at
            # times — see package_commands.py's own use of this same retry
            # helper for the identical issue with stock Servo's .dmg.
            check_call_with_randomized_backoff(
                ["hdiutil", "create", "-volname", package_name, "-srcfolder", stage_dir, dmg_path],
                retries=3,
            )
        except subprocess.CalledProcessError as e:
            print("hdiutil exited with return value %d" % e.returncode)
            raise
        print(f"Packaged into {dmg_path}")

    def _bundle_linux(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_info: BundleLaunchInfo,
    ) -> None:
        """The engine binary itself, copied and renamed to `play`, sitting
        flat in `output_dir` next to its `.so` dependencies (which it now
        finds via the `$ORIGIN` rpath added in `ports/servoshell/build.rs`
        — no `LD_LIBRARY_PATH` wrapper script needed)."""
        play_path = path.join(output_dir, "play")
        shutil.copy(servo_binary, play_path)
        os.chmod(play_path, 0o755)
        for f in os.listdir(binary_dir):
            if ".so" in f:
                shutil.copy(path.join(binary_dir, f), output_dir)
        _write_launch_config(output_dir, launch_info)

    def _bundle_linux_deb(
        self,
        servo_binary: str,
        binary_dir: str,
        output_dir: str,
        launch_info: BundleLaunchInfo,
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
        game_name: Optional[str],
    ) -> None:
        """Build a real, installable .deb: `dpkg -i` puts the engine + its
        content under /usr/lib/<package_name>/, with /usr/bin/<package_name>
        a plain symlink to it (not a wrapper script — the engine now finds
        its own `.so` dependencies via the `$ORIGIN` rpath added in
        `ports/servoshell/build.rs`, and resolves its own launch args
        in-process, so nothing needs to run before it), and a .desktop entry
        so it shows up in application launchers. This is a functional
        package, not a lintian-clean one — no changelog, no man page, no
        maintainer scripts; add those if this ever needs to go through a
        real Debian/Ubuntu review.
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

        binary_name = path.basename(servo_binary)
        shutil.copy(servo_binary, path.join(lib_dir, binary_name))
        os.chmod(path.join(lib_dir, binary_name), 0o755)
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
            game_name,
        )
        _write_launch_config(lib_dir, launch_info)

        # /usr/lib/<package_name> isn't on PATH, so /usr/bin/<package_name>
        # needs to point at the real binary somehow — a symlink is a
        # filesystem alias, not a second executable/process, and
        # `env::current_exe()` (what `bundle_launch.rs` looks next to)
        # resolves through it to the real path on Linux automatically.
        os.symlink(path.join("/usr", "lib", package_name, binary_name), path.join(bin_dir, package_name))

        desktop_entry = f"""[Desktop Entry]
Type=Application
Name={package_name}
Exec=/usr/bin/{package_name}
Terminal=false
Categories=Network;WebBrowser;
"""
        with open(path.join(applications_dir, f"{package_name}.desktop"), "w") as f:
            f.write(desktop_entry)

        # `lstat`, not `getsize`/`stat`: `usr/bin/{package_name}` is now a
        # symlink whose target (`/usr/lib/{package_name}/{binary_name}`) is
        # an absolute path that doesn't exist on *this* (build) machine —
        # only after installation — so following it here would raise
        # `FileNotFoundError`. `lstat` reports the symlink's own (tiny)
        # size instead, matching how `du`/real `dpkg-deb` account for it.
        installed_size_kb = sum(
            os.lstat(path.join(dirpath, name)).st_size
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
