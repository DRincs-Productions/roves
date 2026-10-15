# Copyright 2024 The Servo Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

from typing import TypeGuard
import os
import pathlib
import platform
import shutil
import subprocess
import sys
import toml
from enum import Enum

from os import path
from typing import Any, Optional

import servo.platform
import servo.util as util


class SanitizerKind(Enum):
    NONE = 0
    ASAN = 1
    TSAN = 2

    # Apparently enums don't always compare across modules, so we define
    # helper methods.
    def is_asan(self) -> bool:
        return self is self.ASAN

    def is_tsan(self) -> bool:
        return self is self.TSAN

    # Returns true if no sanitizer is enabled.
    def is_none(self) -> bool:
        return self is self.NONE

    # Returns true if a sanitizer is enabled.
    def is_some(self) -> bool:
        return not self.is_none()


class BuildTarget(object):
    def __init__(self, target_triple: str) -> None:
        self.target_triple = target_triple

    @staticmethod
    def from_triple(target_triple: Optional[str]) -> "BuildTarget":
        host_triple = servo.platform.host_triple()
        if target_triple:
            if "android" in target_triple:
                return AndroidTarget(target_triple)
            elif target_triple != host_triple:
                raise Exception(f"Unknown build target {target_triple}")
        return BuildTarget(host_triple)

    def triple(self) -> str:
        return self.target_triple

    def binary_name(self) -> str:
        return f"servoshell{servo.platform.get().executable_suffix()}"

    def configure_build_environment(self, env: dict[str, str], config: dict[str, Any], topdir: pathlib.Path) -> None:
        pass

    def is_cross_build(self) -> bool:
        return False

    def needs_packaging(self) -> bool:
        return False


class CrossBuildTarget(BuildTarget):
    def is_cross_build(self) -> bool:
        return True


class AndroidTarget(CrossBuildTarget):
    DEFAULT_TRIPLE = "aarch64-linux-android"

    @staticmethod
    def min_sdk() -> int:
        """Minimum supported Android API level"""
        version_catalog_file = path.join(util.SERVO_ROOT, "support", "android", "apk", "gradle", "libs.versions.toml")
        return int(toml.load(version_catalog_file)["versions"]["android-sdk-min"])

    def ndk_configuration(self) -> dict[str, str]:
        target = self.triple()
        api = self.min_sdk()
        config = {}
        if target == "armv7-linux-androideabi":
            config["platform"] = f"android-{api}"
            config["target"] = target
            config["toolchain_prefix"] = "arm-linux-androideabi"
            config["arch"] = "arm"
            config["lib"] = "armeabi-v7a"
            config["toolchain_name"] = f"armv7a-linux-androideabi{api}"
        elif target == "aarch64-linux-android":
            config["platform"] = f"android-{api}"
            config["target"] = target
            config["toolchain_prefix"] = target
            config["arch"] = "arm64"
            config["lib"] = "arm64-v8a"
            config["toolchain_name"] = f"aarch64-linux-androideabi{api}"
        elif target == "i686-linux-android":
            # https://github.com/jemalloc/jemalloc/issues/1279
            config["platform"] = f"android-{api}"
            config["target"] = target
            config["toolchain_prefix"] = target
            config["arch"] = "x86"
            config["lib"] = "x86"
            config["toolchain_name"] = f"i686-linux-android{api}"
        elif target == "x86_64-linux-android":
            config["platform"] = f"android-{api}"
            config["target"] = target
            config["toolchain_prefix"] = target
            config["arch"] = "x86_64"
            config["lib"] = "x86_64"
            config["toolchain_name"] = f"x86_64-linux-android{api}"
        else:
            raise Exception(f"Unknown android target {target}")

        return config

    def configure_build_environment(self, env: dict[str, str], config: dict[str, Any], topdir: pathlib.Path) -> None:
        # Paths to Android build tools:
        if config["android"]["sdk"]:
            env["ANDROID_SDK_ROOT"] = config["android"]["sdk"]
        if config["android"]["ndk"]:
            env["ANDROID_NDK_ROOT"] = config["android"]["ndk"]

        if "IN_NIX_SHELL" in env and ("ANDROID_NDK_ROOT" not in env or "ANDROID_SDK_ROOT" not in env):
            print("Please set SERVO_ANDROID_BUILD=1 when starting the Nix shell to include the Android SDK/NDK.")
            sys.exit(1)
        if "ANDROID_NDK_ROOT" not in env:
            print("Please set the ANDROID_NDK_ROOT environment variable.")
            sys.exit(1)
        if "ANDROID_SDK_ROOT" not in env:
            print("Please set the ANDROID_SDK_ROOT environment variable.")
            sys.exit(1)

        ndk_configuration = self.ndk_configuration()
        android_platform = ndk_configuration["platform"]
        android_toolchain_name = ndk_configuration["toolchain_name"]
        android_api = android_platform.replace("android-", "")

        # Check if the NDK version is 28
        if not os.path.isfile(path.join(env["ANDROID_NDK_ROOT"], "source.properties")):
            print("ANDROID_NDK should have file `source.properties`.")
            print("The environment variable ANDROID_NDK_ROOT may be set at a wrong path.")
            sys.exit(1)
        with open(path.join(env["ANDROID_NDK_ROOT"], "source.properties"), encoding="utf8") as ndk_properties:
            lines = ndk_properties.readlines()
            if lines[1].split(" = ")[1].split(".")[0] != "28":
                print("Servo currently only supports NDK r28.")
                sys.exit(1)

        # Android builds also require having the gcc bits on the PATH and various INCLUDE
        # path munging if you do not want to install a standalone NDK. See:
        # https://dxr.mozilla.org/mozilla-central/source/build/autoconf/android.m4#139-161
        os_type = platform.system().lower()
        if os_type not in ["linux", "darwin"]:
            raise Exception("Android cross builds are only supported on Linux and macOS.")

        llvm_prebuilt = path.join(env["ANDROID_NDK_ROOT"], "toolchains", "llvm", "prebuilt")

        cpu_type = platform.machine().lower()
        host_suffix = "unknown"
        if cpu_type in ["i386", "i486", "i686", "i768", "x86"]:
            host_suffix = "x86"
        elif cpu_type in ["x86_64", "x86-64", "x64", "amd64"]:
            host_suffix = "x86_64"
        else:
            available_prebuilts = os.listdir(llvm_prebuilt)
            available_prebuilts = [prebuilt for prebuilt in available_prebuilts if prebuilt.startswith(os_type)]
            # If there is only one prebuilt option available, it's probably the right one for the host
            # platform. E.g. on Arm macs, only the x86 prebuilts are available, buts that perfectly fine,
            # since there is rosetta.
            if len(available_prebuilts) == 1:
                host_suffix = available_prebuilts[0].removeprefix(f"{os_type}-")
            else:
                print(f"Error: Can't determine LLVM prebuilt. Unknown cpu type {cpu_type}.")
                print(f"Hint: The LLVM prebuilts folder contains the following entries: {available_prebuilts}")
                print("Please open an issue with the above information")
                raise Exception("Can't determine LLVM prebuilt directory.")
        host = os_type + "-" + host_suffix

        host_cc = env.get("HOST_CC") or shutil.which("clang")
        host_cxx = env.get("HOST_CXX") or shutil.which("clang++")

        llvm_toolchain = path.join(llvm_prebuilt, host)
        env["PATH"] = env["PATH"] + ":" + path.join(llvm_toolchain, "bin")

        def to_ndk_bin(prog: str) -> str:
            return path.join(llvm_toolchain, "bin", prog)

        # This workaround is due to an issue in the x86_64 Android NDK that introduces
        # an undefined reference to the symbol '__extendsftf2'.
        # See https://github.com/termux/termux-packages/issues/8029#issuecomment-1369150244
        if "x86_64" in self.triple():
            libclangrt_filename = subprocess.run(
                [to_ndk_bin(f"x86_64-linux-android{android_api}-clang"), "--print-libgcc-file-name"],
                check=True,
                capture_output=True,
                encoding="utf8",
            ).stdout
            env["RUSTFLAGS"] = env.get("RUSTFLAGS", "")
            env["RUSTFLAGS"] += f" -C link-arg={libclangrt_filename}"

        assert host_cc
        assert host_cxx

        env["RUST_TARGET"] = self.triple()
        env["HOST_CC"] = host_cc
        env["HOST_CXX"] = host_cxx
        env["HOST_CFLAGS"] = ""
        env["HOST_CXXFLAGS"] = ""
        env["TARGET_CC"] = to_ndk_bin("clang")
        env["TARGET_CPP"] = to_ndk_bin("clang") + " -E"
        env["TARGET_CXX"] = to_ndk_bin("clang++")

        env["TARGET_AR"] = to_ndk_bin("llvm-ar")
        env["TARGET_RANLIB"] = to_ndk_bin("llvm-ranlib")
        env["TARGET_OBJCOPY"] = to_ndk_bin("llvm-objcopy")
        env["TARGET_YASM"] = to_ndk_bin("yasm")
        env["TARGET_STRIP"] = to_ndk_bin("llvm-strip")
        env["RUST_FONTCONFIG_DLOPEN"] = "on"

        # Needed for tikv-jemalloc, which doesn't respect TARGET_AR and co.
        # On macos this lead to it falling back to `ar` and missing jemalloc
        # symbols in libservoshell.so.
        env["AR"] = to_ndk_bin("llvm-ar")
        env["RANLIB"] = to_ndk_bin("llvm-ranlib")

        env["LIBCLANG_PATH"] = path.join(llvm_toolchain, "lib")
        env["CLANG_PATH"] = to_ndk_bin("clang")
        env["BINDGEN_EXTRA_CLANG_ARGS"] = (
            f"--target={android_toolchain_name} --sysroot={path.join(llvm_toolchain, 'sysroot')}"
        )

        # A cheat-sheet for some of the build errors caused by getting the search path wrong...
        #
        # fatal error: 'limits' file not found
        #   -- add -I cxx_include
        # unknown type name '__locale_t' (when running bindgen on C++ headers)
        #   -- add -isystem sysroot_include
        # error: use of undeclared identifier 'UINTMAX_C'
        #   -- add -D__STDC_CONSTANT_MACROS
        #
        # Also worth remembering: autoconf uses C for its configuration,
        # even for C++ builds, so the C flags need to line up with the C++ flags.
        env["TARGET_CFLAGS"] = env.get("TARGET_CFLAGS", "") + " " + "--target=" + android_toolchain_name
        env["TARGET_CXXFLAGS"] = env.get("TARGET_CXXFLAGS", "") + " " + "--target=" + android_toolchain_name

        # Native build scripts read these two variables.
        env["ANDROID_API_LEVEL"] = android_api
        env["ANDROID_NDK_HOME"] = env["ANDROID_NDK_ROOT"]
        env["TARGET_PKG_CONFIG_SYSROOT_DIR"] = path.join(llvm_toolchain, "sysroot")

    def binary_name(self) -> str:
        return "libservoshell.so"

    def is_cross_build(self) -> bool:
        return True

    def needs_packaging(self) -> bool:
        return True

    def get_package_path(self, build_type_directory: str) -> str:
        base_path = util.get_target_dir()
        base_path = path.join(base_path, self.triple())
        apk_name = "servoapp.apk"
        return path.join(base_path, build_type_directory, apk_name)


def is_android(target: BuildTarget) -> TypeGuard[AndroidTarget]:
    return isinstance(target, AndroidTarget)
