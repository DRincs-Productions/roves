"""Extracts bindgen type items (struct/union/enum/type/const) from mozjs_sys's jsapi.rs.

Usage: extract_types.py <jsapi.rs> <out.rs> NAME [NAME ...]

Items are looked up by name across bindgen's `root`, `root::JS`, `root::js`, ... modules,
dependencies referenced as `root::...::Name` are pulled in transitively, and paths are
flattened to bare names. Types listed in OPAQUE become zero-sized opaque structs.
"""

import re
import sys

# Provided by roves-js itself (never emitted; the generated file imports them).
EXTERNAL = {
    "Value", "JSVal", "JSContext", "JSObject", "JSString", "Symbol", "BigInt", "JSFunction", "JSScript",
    "Heap", "JS_CALLEE", "StackGCVector", "PropertyKey", "jsid", "Rooted",
}

OPAQUE = {
    # SpiderMonkey internals the emulation never inspects.
    "ClassSpec", "ClassExtension", "Realm", "Compartment", "Zone", "GCContext",
    "JSRuntime", "JSAtom", "JSLinearString", "InlinableNative", "TrampolineNative", "JSTracer", "JSPrincipals",
    "BaseProxyHandler",
    "RootingContext", "AutoRequireNoGC",
    # C++ classes and containers behind pointers.
    "BuildIdCharVector", "JobQueue", "StreamConsumer", "ScriptEnvironmentPreparer_Closure",
}


def split_items(text):
    """Yields (module_path, name, kind, item_text) for every bindgen item."""
    lines = text.split("\n")
    path = []
    depth = 0
    module_depths = []
    pending_attrs = []
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        mod_match = re.match(r"pub mod (\w+) \{$", stripped)
        if mod_match:
            path.append(mod_match.group(1))
            depth += 1
            module_depths.append(depth)
            pending_attrs = []
            i += 1
            continue
        if stripped == "}" and module_depths and depth == module_depths[-1]:
            module_depths.pop()
            path.pop()
            depth -= 1
            i += 1
            continue
        if stripped.startswith("#["):
            pending_attrs.append(line)
            i += 1
            continue
        impl_match = re.match(r"impl(?:<[^>]*>)? (\w+)(?:<[^>]*>)?\s*(?:where\b.*)?(\{)?$", stripped)
        if impl_match and " for " not in stripped:
            # bindgen's inherent impls (bitfield accessors, constructors): emitted with their type.
            name = impl_match.group(1)
            body = [line]
            balance = line.count("{") - line.count("}")
            j = i + 1
            while (balance > 0 or "{" not in "".join(body)) and j < len(lines):
                body.append(lines[j])
                balance += lines[j].count("{") - lines[j].count("}")
                j += 1
            yield (tuple(path), name, "impl", "\n".join(pending_attrs + body))
            pending_attrs = []
            i = j
            continue
        item = re.match(r"pub (struct|union|enum|type|const) (\w+)", stripped)
        if item:
            kind, name = item.group(1), item.group(2)
            body = [line]
            balance = line.count("{") - line.count("}")
            ends = stripped.endswith(";") and balance == 0
            j = i + 1
            while not ends and j < len(lines):
                body.append(lines[j])
                balance += lines[j].count("{") - lines[j].count("}")
                if balance <= 0 and (lines[j].strip().endswith("}") or lines[j].strip().endswith(";")):
                    ends = True
                j += 1
            yield (tuple(path), name, kind, "\n".join(pending_attrs + body))
            pending_attrs = []
            i = j
            continue
        if stripped and not stripped.startswith("//"):
            pending_attrs = []
        i += 1


def main():
    source, output, *names = sys.argv[1:]
    text = open(source, encoding="utf-8").read()
    index = {}
    impls = {}
    for path, name, kind, item in split_items(text):
        if kind == "impl":
            impls.setdefault(name, []).append(item)
        else:
            index.setdefault(name, []).append((path, kind, item))

    def lookup(name):
        candidates = index.get(name, [])
        # bindgen emits a placeholder for enums it could not see the body of (a forward
        # declaration in another namespace); prefer the full definition.
        full = [c for c in candidates if "__bindgen_cannot_repr_c_on_empty_enum" not in c[2]]
        candidates = full or candidates
        for preferred in (("root",), ("root", "JS"), ("root", "js")):
            for path, kind, item in candidates:
                if path == preferred:
                    return kind, item
        return (candidates[0][1], candidates[0][2]) if candidates else None

    emitted = {}
    queue = list(names)
    missing = []
    while queue:
        name = queue.pop()
        if name in emitted or name in EXTERNAL:
            continue
        if name in OPAQUE:
            emitted[name] = f"#[repr(C)]\n#[derive(Debug, Copy, Clone, PartialEq)]\npub struct {name} {{\n    _private: [u8; 0],\n}}"
            continue
        found = lookup(name)
        if not found:
            missing.append(name)
            continue
        kind, item = found
        # Drop bindgen's layout assertions and derives that pull in unsupported traits.
        item = re.sub(r"root::(\w+::)*", "", item)
        item = item.replace("::std::os::raw::", "std::os::raw::")
        emitted[name] = item
        for dependency in set(re.findall(r"\b([A-Z_][A-Za-z0-9_]+)\b", item)):
            if dependency != name and dependency in index and dependency not in emitted and dependency not in EXTERNAL:
                queue.append(dependency)
    with open(output, "w", encoding="utf-8", newline="") as out:
        out.write("// Generated by extract_types.py from mozjs_sys's bindgen output.\n\n")
        for name in sorted(emitted):
            out.write(emitted[name] + "\n\n")
            if name not in OPAQUE:
                for block in impls.get(name, []):
                    block = re.sub(r"root::(\w+::)*", "", block)
                    block = block.replace("::std::os::raw::", "std::os::raw::")
                    out.write(block + "\n\n")
    print(f"emitted {len(emitted)} items; missing: {sorted(set(missing))}")


if __name__ == "__main__":
    main()
