#!/usr/bin/env sh
# Downloads the exact crates.io releases of wgpu / wgpu-core used by the proof,
# verifies their checksums (the same ones Cargo.lock records for the registry
# crates) and applies upstream/wgpu-29-core-interop.patch.
#   PRISTINE=1 ./prepare-patched.sh   -> unpatched sources (compile repro)
set -eu
cd "$(dirname "$0")"
rm -rf patched && mkdir -p patched/download
fetch() { # name version sha256 dir
  file="patched/download/$1-$2.crate"
  curl -fsSL "https://static.crates.io/crates/$1/$1-$2.crate" -o "$file"
  echo "$3  $file" | sha256sum -c -
  tar -xzf "$file" -C patched
  mv "patched/$1-$2" "patched/$4"
}
fetch wgpu 29.0.4 76e8840e1ba2881d4cbb18d2147627a56af426ff064c0401eb0c8410c6325d07 wgpu
fetch wgpu-core 29.0.1 1e80ac6cf1895df6342f87d975162108f9d98772a0d74bc404ab7304ac29469e wgpu-core
if [ "${PRISTINE:-0}" != 1 ]; then
  # `patch`, not `git apply`: inside the Roves checkout git ignores these paths.
  (cd patched && patch -p1 --forward < ../upstream/wgpu-29-core-interop.patch)
fi
