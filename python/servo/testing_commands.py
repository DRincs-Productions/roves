# Copyright 2013 The Servo Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import os
import os.path as path
import shutil
from contextlib import chdir
from typing import Any, List, Optional

from mach.decorators import (
    Command,
    CommandArgument,
    CommandProvider,
)

from servo.command_base import BuildType, CommandBase, call

# Servo depends on several `rustfmt` options that are unstable. These are still
# supported by stable `rustfmt` if they are passed as these command-line arguments.
UNSTABLE_RUSTFMT_ARGUMENTS = [
    "--config",
    "unstable_features=true",
    "--config",
    "binop_separator=Back",
    "--config",
    "imports_granularity=Module",
    "--config",
    "group_imports=StdExternalCrate",
]

# Listing these globs manually is a work-around for very slow `taplo` invocation
# on MacOS machines. If `taplo` runs fast without the globs on MacOS, this
# can be removed.
TOML_GLOBS = [
    "*.toml",
    ".cargo/*.toml",
    "components/*/*.toml",
    "components/*/*/*.toml",
    "components/media/backends/*/*.toml",
    "components/media/*/*/*/*.toml",
    "ports/*/*.toml",
    "support/*/*.toml",
    "tests/unit/*/*.toml",
]


def format_toml_files_with_taplo(check_only: bool = True) -> int:
    taplo = shutil.which("taplo")
    if taplo is None:
        print("Could not find `taplo`. Run `./mach bootstrap` or `cargo install taplo-cli --locked`")
        return 1

    if check_only:
        return call([taplo, "fmt", "--check", *TOML_GLOBS], env={"RUST_LOG": "error"})
    else:
        return call([taplo, "fmt", *TOML_GLOBS], env={"RUST_LOG": "error"})


def format_python_files_with_ruff(check_only: bool = True) -> int:
    ruff = shutil.which("ruff")
    if ruff is None:
        print("Could not find `ruff`. Run `./mach bootstrap`")
        return 1

    if check_only:
        return call([ruff, "format", "--check", "--quiet"])
    else:
        return call([ruff, "format", "--quiet"])


def format_with_rustfmt(check_only: bool = True) -> int:
    maybe_check_only = ["--check"] if check_only else []
    result = call(["cargo", "fmt", "--", *UNSTABLE_RUSTFMT_ARGUMENTS, *maybe_check_only])
    if result != 0:
        return result

    return call(
        [
            "cargo",
            "fmt",
            "--manifest-path",
            "support/crown/Cargo.toml",
            "--",
            *UNSTABLE_RUSTFMT_ARGUMENTS,
            *maybe_check_only,
        ]
    )


@CommandProvider
class MachCommands(CommandBase):
    @Command("test-unit", description="Run unit tests", category="testing")
    @CommandArgument("--package", "-p", default=None, help="Specific package to test")
    @CommandArgument("--bench", default=False, action="store_true", help="Run in bench mode")
    @CommandArgument(
        "--nocapture", default=False, action="store_true", help="Run tests with nocapture ( show test stdout )"
    )
    @CommandArgument("--code-coverage", default=False, action="store_true", help="Run in code coverage mode")
    @CommandArgument("--llvm-cov-option", default=None, action="append", help="Additional options for llvm-cov")
    @CommandArgument("--nextest-profile", default=None, help="Specify the Nextest profile to use")
    @CommandArgument("params", nargs="...", help="Command-line arguments to be passed through to Cargo nextest")
    @CommandBase.common_command_arguments(build_configuration=True, build_type=True)
    def test_unit(
        self,
        build_type: BuildType,
        params: list[str] | None = None,
        package: str | None = None,
        bench: bool = False,
        code_coverage: bool = False,
        llvm_cov_option: Optional[List[str]] = None,
        nocapture: bool = False,
        nextest_profile: str | None = None,
        **kwargs: Any,
    ) -> int:
        self.ensure_bootstrapped()

        if package:
            packages = {package}
        else:
            packages = set()

        self_contained_tests = [
            "servo-background-hang-monitor",
            "servo-base",
            "servo-constellation",
            "servo-devtools",
            "servo-fonts",
            "servo-hyper-serde",
            "servo-layout",
            "servo-layout-api",
            "servo",
            "servo-metrics",
            "servo-net",
            "servo-net-traits",
            "servo-paint",
            "servo-paint-api",
            "servo-pixels",
            "servo-script-traits",
            "servo-script-bindings",
            "selectors",
            "servo-config",
            "servoshell",
            "servo-url",
            "servo-storage",
            "servo-storage-traits",
            "servo-xpath",
            "servo-deny-public-fields",
            "servo-dom-struct",
            "servo-webvtt",
        ]
        if not packages:
            packages = set(os.listdir(path.join(self.context.topdir, "tests", "unit"))) - set([".DS_Store"])
            packages |= set(self_contained_tests)

        in_crate_packages = []
        for crate in self_contained_tests:
            try:
                packages.remove(crate)
                in_crate_packages += [crate]
            except KeyError:
                pass

        # Return if there is nothing to do.
        if len(packages) == 0 and len(in_crate_packages) == 0:
            return 0

        args: list[str] = params or []
        use_nextest = "--doc" not in args

        if build_type.is_release():
            args += ["--release"]
        elif build_type.is_dev():
            pass  # there is no argument for debug
        else:
            cargo_profile_arg = "--cargo-profile" if use_nextest else "--profile"
            args += [cargo_profile_arg, build_type.profile]

        if use_nextest and nextest_profile is not None:
            args += ["--profile", nextest_profile]

        for crate in packages:
            args += ["-p", "%s_tests" % crate]
        for crate in in_crate_packages:
            args += ["-p", crate]

        if nocapture:
            args += ["--nocapture"]
        args += ["--no-fail-fast"]

        env = self.build_env()

        crown_cargo_command: List[str] = ["cargo"]
        cargo_command: str
        if bench:
            cargo_command = "bench"
            if code_coverage:
                print(
                    "Error: Invalid argument combination for `./mach test-unit`. "
                    "`--bench` and `--code-coverage` are mutually exclusive."
                )
                exit(1)
        elif code_coverage:
            if not use_nextest:
                print(
                    "Error: Invalid argument combination for `./mach test-unit`. "
                    "`--doc` and `--code-coverage` are mutually exclusive."
                )
                exit(1)
            cargo_llvm_cov_options: List[str] = llvm_cov_option or []
            crown_cargo_command.extend(["llvm-cov", "nextest"])
            crown_cargo_command.extend(cargo_llvm_cov_options)
            cargo_command = "llvm-cov"
            args.insert(0, "nextest")
            args.extend(cargo_llvm_cov_options)
        elif use_nextest:
            crown_cargo_command.extend(["nextest", "run"])
            cargo_command = "nextest"
            args.insert(0, "run")
        else:
            crown_cargo_command.extend(["test"])
            cargo_command = "test"
        result = call(crown_cargo_command, cwd="support/crown")
        if result != 0:
            return result
        result = self.run_cargo_build_like_command(cargo_command, args, env=env, **kwargs)
        assert isinstance(result, int)
        return result

    @Command("fmt", description="Format Rust, Python, and TOML files", category="testing")
    def format_code(self) -> int:
        with chdir(self.context.topdir):
            result = format_python_files_with_ruff(check_only=False)
            if result != 0:
                return result

            result = format_toml_files_with_taplo(check_only=False)
            if result != 0:
                return result

            return format_with_rustfmt(check_only=False)
