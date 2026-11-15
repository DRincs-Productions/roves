"""Pin, provenance and log checks for the shared-device proof CI job.

Run with `python -I ci/verify.py <command> ...` from the fixture directory.
Commands:
  pins                       Cargo.toml / Cargo.lock / prepare-patched.sh pins.
  crates-io                  prepare-patched.sh sha256 == crates.io index cksum.
  patched-diff PRISTINE PATCHED
                             the patched tree differs from the pristine
                             crates.io sources only in files named by
                             upstream/wgpu-29-core-interop.patch.
  lock-diff ORIGINAL NEW     the positive run's lock (Netrender path override)
                             changes only the source of the Netrender git
                             packages; every other package is identical.
  log negative|positive raw|deno lavapipe|warp LOG STATUS
                             the run failed/passed for the expected reason
                             on the expected software adapter.
"""

import json
import os
import re
import sys
import tomllib
import urllib.request

NETRENDER_REV = "d2f13ca5ecd20df89d148a7cebef28c0962b90ca"
NETRENDER_GIT = "git+https://github.com/merely-made/netrender"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
# name -> exact version; every one must appear exactly once in Cargo.lock.
LOCKED = {
    "wgpu": "29.0.4",
    "wgpu-core": "29.0.1",
    "wgpu-hal": "29.0.4",
    "wgpu-types": "29.0.1",
    "naga": "29.0.4",
    "vello": "0.9.0",
    "netrender": "0.1.1",
    "v8": "150.4.0",
    "deno_core": "0.413.0",
    "deno_web": "0.291.0",
    "deno_webidl": "0.260.0",
    "deno_webgpu": "0.227.0",
}
# The two crates replaced by `[patch.crates-io]` path entries (patched/).
PATCHED_CRATES = {"wgpu": "29.0.4", "wgpu-core": "29.0.1"}
# Software adapters the `log` command accepts: kind -> (AdapterInfo name
# substring, AdapterInfo backend). CI runs on lavapipe (Mesa llvmpipe, Vulkan);
# WARP is kept for local runs (Vello's compute frame crashes it, see README).
ADAPTERS = {
    "lavapipe": ("llvmpipe", "Vulkan"),
    "warp": ("Microsoft Basic Render Driver", "Dx12"),
}
EXPECTED_FAILS = [
    "after layer over before layer (50,10)",
    "after layer over 50% canvas over before layer (40,40)",
]
ALL_CHECKS = [
    "before layer only (4,4)",
    "opaque canvas top half (20,20)",
    "50% canvas over before layer (20,40)",
    "after layer over opaque canvas (40,20)",
] + EXPECTED_FAILS


def fail(message):
    print(f"::error::{message}")
    sys.exit(1)


def load_lock(path):
    with open(path, "rb") as f:
        return tomllib.load(f)["package"]


def prepare_pins():
    with open("prepare-patched.sh", encoding="utf-8") as f:
        script = f.read()
    pins = re.findall(r"^fetch (\S+) (\S+) ([0-9a-f]{64}) (\S+)$", script, re.M)
    if {(n, v) for n, v, _, _ in pins} != set(PATCHED_CRATES.items()):
        fail(f"prepare-patched.sh fetches {pins}, expected {PATCHED_CRATES}")
    return {n: (v, sha) for n, v, sha, _ in pins}


def cmd_pins():
    with open("Cargo.toml", "rb") as f:
        manifest = tomllib.load(f)
    deps = manifest["dependencies"]
    if deps["wgpu-core"]["version"] != "=29.0.1" or deps["wgpu"]["version"] != "=29.0.4":
        fail("Cargo.toml no longer pins wgpu-core =29.0.1 / wgpu =29.0.4")
    if deps["netrender"].get("rev") != NETRENDER_REV:
        fail(f"Cargo.toml netrender rev is {deps['netrender'].get('rev')}")
    for name in ("deno_core", "deno_web", "deno_webidl", "deno_webgpu"):
        if deps[name]["version"] != "=" + LOCKED[name]:
            fail(f"Cargo.toml pins {name} {deps[name]['version']}")
    patch = manifest["patch"]["crates-io"]
    if patch != {"wgpu": {"path": "patched/wgpu"}, "wgpu-core": {"path": "patched/wgpu-core"}}:
        fail(f"unexpected [patch.crates-io]: {patch}")
    packages = load_lock("Cargo.lock")
    for name, version in LOCKED.items():
        found = [p for p in packages if p["name"] == name]
        if [p["version"] for p in found] != [version]:
            fail(f"Cargo.lock has {name} {[p['version'] for p in found]}, expected exactly [{version}]")
        source = found[0].get("source")
        if name in PATCHED_CRATES:
            if source is not None:
                fail(f"{name} is not the [patch] path crate (source {source})")
        elif name == "netrender":
            if source != f"{NETRENDER_GIT}?rev={NETRENDER_REV}#{NETRENDER_REV}":
                fail(f"netrender source {source}")
        elif source != REGISTRY:
            fail(f"{name} comes from {source}")
    for p in packages:
        source = p.get("source") or ""
        if source.startswith(NETRENDER_GIT) and not source.endswith("#" + NETRENDER_REV):
            fail(f"{p['name']} locked at {source}")
        if source == "" and p["name"] not in PATCHED_CRATES and p["name"] != manifest["package"]["name"]:
            fail(f"unexpected path package {p['name']} in Cargo.lock")
    prepare_pins()
    print("pins OK: " + ", ".join(f"{n} {v}" for n, v in LOCKED.items()) + f", netrender {NETRENDER_REV[:8]}")


def cmd_crates_io():
    for name, (version, sha) in prepare_pins().items():
        url = f"https://index.crates.io/{name[:2]}/{name[2:4]}/{name}"
        request = urllib.request.Request(url, headers={"User-Agent": "netrender-proof-ci"})
        with urllib.request.urlopen(request, timeout=60) as response:
            entries = [json.loads(line) for line in response.read().decode().splitlines() if line]
        entry = next((e for e in entries if e["vers"] == version), None)
        if entry is None or entry.get("yanked"):
            fail(f"{name} {version} missing or yanked on crates.io")
        if entry["cksum"] != sha:
            fail(f"{name} {version}: crates.io cksum {entry['cksum']} != prepare-patched.sh {sha}")
        print(f"crates.io {name} {version} cksum matches ({sha[:16]}...)")


def tree_files(root):
    out = {}
    for base, _, files in os.walk(root):
        for name in files:
            path = os.path.join(base, name)
            with open(path, "rb") as f:
                out[os.path.relpath(path, root).replace(os.sep, "/")] = f.read()
    return out


def cmd_patched_diff(pristine, patched):
    with open("upstream/wgpu-29-core-interop.patch", encoding="utf-8") as f:
        named = set(re.findall(r"^\+\+\+ b/(\S+)$", f.read(), re.M))
    changed = set()
    for crate in PATCHED_CRATES:
        a = tree_files(os.path.join(pristine, crate))
        b = tree_files(os.path.join(patched, crate))
        for rel in set(a) | set(b):
            if a.get(rel) != b.get(rel):
                changed.add(f"{crate}/{rel}")
    if changed != named:
        fail(f"patched tree differs in {sorted(changed)}, the patch names {sorted(named)}")
    print("patched tree = crates.io sources + upstream patch only: " + ", ".join(sorted(changed)))


def key(p):
    return (p["name"], p["version"])


def cmd_lock_diff(original, new):
    a = {key(p): p for p in load_lock(original)}
    b = {key(p): p for p in load_lock(new)}
    if set(a) != set(b):
        fail(f"positive lock changed packages: -{sorted(set(a) - set(b))} +{sorted(set(b) - set(a))}")
    moved = []
    for k, pa in a.items():
        pb = b[k]
        if pa == pb:
            continue
        if (pa.get("source") or "").startswith(NETRENDER_GIT) and pb.get("source") is None:
            if {x: y for x, y in pa.items() if x != "source"} == pb:
                moved.append(k[0])
                continue
        fail(f"positive lock changed {k}: {pa} -> {pb}")
    if "netrender" not in moved:
        fail("the positive run did not use the local Netrender checkout")
    print(f"positive lock: only {sorted(moved)} moved from git to the patched checkout")


def cmd_log(kind, producer, adapter_kind, log, status):
    with open(log, encoding="utf-8", errors="replace") as f:
        text = f.read()
    status = int(status)
    if adapter_kind not in ADAPTERS:
        fail(f"unknown adapter kind {adapter_kind}, expected one of {sorted(ADAPTERS)}")

    def fail_with_tail(message):
        # Raw job logs need repository admin rights; annotations do not.
        for line in text.splitlines()[-8:]:
            print(f"::notice::{kind} {producer} log tail: {line[:300]}")
        fail(message)

    if "panicked" in text:
        fail_with_tail(f"{kind} {producer}: panic (not a pixel failure): see log")
    if status not in (0, 1):
        fail_with_tail(f"{kind} {producer}: exit {status} (crash or timeout, not a pixel result)")
    adapter = re.search(r"^adapter: .*$", text, re.M)
    if not adapter:
        fail_with_tail(f"{kind} {producer}: no adapter line, the run did not reach Netrender")
    print(f"::notice::{kind} {producer}: {adapter.group(0)[:300]}")
    name, backend = ADAPTERS[adapter_kind]
    if name not in adapter.group(0) or f"backend: {backend}" not in adapter.group(0) \
            or "device_type: Cpu" not in adapter.group(0):
        fail_with_tail(f"{kind} {producer}: not the {adapter_kind} adapter ({name}, {backend}, Cpu)")
    shared = re.search(r"^hub ids before Netrender .*$", text, re.M)
    if not shared:
        fail_with_tail(f"{kind} {producer}: no shared-device hub line")
    print(f"::notice::{kind} {producer}: {shared.group(0)[:300]}")
    if producer == "raw" and "device ids: 2 with the clones, 1 after Deno dropped its own" not in text:
        fail_with_tail("raw: Deno-side id release not reported")
    if producer == "deno":
        for needle in ("guest: no privileged global visible", "hub ids: guest only", "hub ids after "):
            if needle not in text:
                fail_with_tail(f"deno: missing '{needle}'")
        gc = re.search(r"^hub ids after (\d+) GC pass\(es\).*$", text, re.M)
        print(f"::notice::{kind} deno: {gc.group(0)[:300]}")
    results = dict(
        (m.group(2), m.group(1))
        for m in re.finditer(r"^(PASS|FAIL) (.+?): actual", text, re.M)
    )
    for m in re.finditer(r"^(FAIL .*)$", text, re.M):
        print(f"::notice::{kind} {producer}: {m.group(1)[:300]}")
    if sorted(results) != sorted(ALL_CHECKS):
        fail_with_tail(f"{kind} {producer}: pixel checks {sorted(results)}")
    failed = sorted(name for name, r in results.items() if r == "FAIL")
    if kind == "negative":
        if status != 1 or failed != sorted(EXPECTED_FAILS) or "2 pixel assertion(s) failed" not in text:
            fail_with_tail(f"{kind} {producer}: exit {status}, failed {failed}; expected exit 1 with exactly {EXPECTED_FAILS}")
    else:
        if status != 0 or failed or "all assertions passed" not in text:
            fail_with_tail(f"{kind} {producer}: exit {status}, failed {failed}; expected exit 0 and 6/6")
    print(f"{kind} {producer}: exit {status}, {6 - len(failed)}/6 pixels pass as expected")


def main(argv):
    commands = {
        "pins": (cmd_pins, 0),
        "crates-io": (cmd_crates_io, 0),
        "patched-diff": (cmd_patched_diff, 2),
        "lock-diff": (cmd_lock_diff, 2),
        "log": (cmd_log, 5),
    }
    if not argv or argv[0] not in commands or len(argv) - 1 != commands[argv[0]][1]:
        print(__doc__)
        sys.exit(2)
    commands[argv[0]][0](*argv[1:])


if __name__ == "__main__":
    main(sys.argv[1:])
