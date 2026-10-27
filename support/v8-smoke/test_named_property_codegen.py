"""Named-property visibility in the generated proxy handlers ([LegacyOverrideBuiltIns]).

Runs the vendored WebIDL parser and the real binding generator on a small IDL and checks the
order of the lookups in the generated get, getOwnPropertyDescriptor and hasOwn hooks:
https://webidl.spec.whatwg.org/#dfn-named-property-visibility
"""

from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINDINGS = ROOT / "components" / "script_bindings"
for path in (BINDINGS / "third_party" / "ply", BINDINGS / "third_party" / "WebIDL" / "parser", BINDINGS / "codegen"):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

import WebIDL  # noqa: E402
from configuration import Configuration  # noqa: E402
from codegen import (  # noqa: E402
    CGDOMJSProxyHandler_get,
    CGDOMJSProxyHandler_getOwnPropertyDescriptor,
    CGDOMJSProxyHandler_hasOwn,
)

IDL = """
[Global=Window, Exposed=Window] interface Window {};
[Exposed=Window, LegacyOverrideBuiltIns] interface OverrideNamed {
  getter DOMString (DOMString name);
};
[Exposed=Window] interface PlainNamed {
  getter DOMString (DOMString name);
};
[Exposed=Window, LegacyOverrideBuiltIns] interface OverrideIndexed {
  getter DOMString (unsigned long index);
  getter DOMString (DOMString name);
  readonly attribute unsigned long length;
};
[Exposed=Window] interface PlainIndexed {
  getter DOMString (unsigned long index);
  getter DOMString (DOMString name);
  readonly attribute unsigned long length;
};
"""
# The real HTMLDocument.webidl, with stubs for what it references from Document.webidl.
HTML_DOCUMENT_STUBS = """
[Exposed=Window] interface Document {};
typedef DOMString NamedPropertyValue;
"""
CONFIG = "DOMInterfaces = {}\nEnums = {}\nDictionaries = {}\nUnions = {}\nSubCrates = {'script_webgpu': []}\n"

NAMED = "NamedGetter("
INDEXED = "IndexedGetter("
# Real IDL interfaces with [LegacyOverrideBuiltIns] are included in every override case.
OVERRIDE = ("OverrideNamed", "OverrideIndexed", "HTMLDocument", "DOMStringMap")
PLAIN = ("PlainNamed", "PlainIndexed")


def configuration():
    with tempfile.TemporaryDirectory() as scratch:
        parser = WebIDL.Parser(scratch)
        parser.parse(IDL, "NamedPropertyVisibility.webidl")
        parser.parse(HTML_DOCUMENT_STUBS, "Stubs.webidl")
        for name in ("HTMLDocument.webidl", "DOMStringMap.webidl"):
            path = BINDINGS / "webidls" / name
            parser.parse(path.read_text(encoding="utf-8"), str(path))
        results = parser.finish()
        config_file = Path(scratch) / "Bindings.conf"
        config_file.write_text(CONFIG, encoding="utf-8")
        return Configuration(str(config_file), results)


CONFIG_CACHE = None


def descriptor(name):
    global CONFIG_CACHE
    if CONFIG_CACHE is None:
        CONFIG_CACHE = configuration()
    return CONFIG_CACHE.getDescriptor(name)


def bodies(name):
    d = descriptor(name)
    return {
        "get": CGDOMJSProxyHandler_get(d).getBody(),
        "getOwnPropertyDescriptor": CGDOMJSProxyHandler_getOwnPropertyDescriptor(d).getBody(),
        "hasOwn": CGDOMJSProxyHandler_hasOwn(d).getBody(),
    }


class NamedPropertyVisibilityTests(unittest.TestCase):
    def assertBefore(self, body, first, second):
        self.assertIn(first, body)
        self.assertIn(second, body)
        self.assertLess(body.index(first), body.index(second), f"{first!r} must come before {second!r}")

    def test_extended_attribute_is_read_from_the_idl(self):
        for name in OVERRIDE:
            d = descriptor(name)
            self.assertTrue(d.proxy and d.supportsNamedProperties(), name)
            self.assertTrue(d.interface.getExtendedAttribute("LegacyOverrideBuiltIns"), name)
        for name in PLAIN + ("Document",):
            self.assertFalse(descriptor(name).interface.getExtendedAttribute("LegacyOverrideBuiltIns"), name)

    def test_override_get_reads_named_before_the_prototype(self):
        for name in OVERRIDE:
            body = bodies(name)["get"]
            self.assertBefore(body, "get_expando_object(", NAMED)
            self.assertBefore(body, NAMED, "get_property_on_prototype(")

    def test_plain_get_lets_the_prototype_hide_named(self):
        for name in PLAIN:
            body = bodies(name)["get"]
            self.assertBefore(body, "get_expando_object(", "get_property_on_prototype(")
            self.assertBefore(body, "get_property_on_prototype(", NAMED)

    def test_override_own_property_hooks_skip_the_prototype(self):
        for name in OVERRIDE:
            hooks = bodies(name)
            for hook, expando in (("getOwnPropertyDescriptor", "JS_GetPropertyDescriptorById("),
                                  ("hasOwn", "JS_HasPropertyById(")):
                body = hooks[hook]
                self.assertNotIn("has_property_on_prototype", body, f"{name}.{hook}")
                self.assertBefore(body, "get_expando_object(", NAMED)
                self.assertBefore(body, expando, NAMED)

    def test_plain_own_property_hooks_check_the_prototype(self):
        for name in PLAIN:
            for hook in ("getOwnPropertyDescriptor", "hasOwn"):
                body = bodies(name)[hook]
                self.assertBefore(body, "get_expando_object(", "has_property_on_prototype(")
                self.assertBefore(body, "has_property_on_prototype(", NAMED)

    def test_indexed_properties_stay_first_and_exclude_named(self):
        for name in ("OverrideIndexed", "PlainIndexed"):
            for hook, body in bodies(name).items():
                self.assertBefore(body, INDEXED, NAMED)
                self.assertIn("index.is_none() && (id.is_string() || id.is_int())", body, f"{name}.{hook}")
            # get forwards a missing index neither to the expando nor to the named getter.
            self.assertBefore(bodies(name)["get"], INDEXED, "get_expando_object(")

    def test_plain_output_matches_the_previous_shape(self):
        # Without the attribute the named lookup stays after the prototype check, once per hook.
        for name in PLAIN:
            hooks = bodies(name)
            self.assertEqual(1, hooks["get"].count("get_property_on_prototype("))
            self.assertEqual(1, hooks["getOwnPropertyDescriptor"].count("has_property_on_prototype("))
            self.assertEqual(1, hooks["hasOwn"].count("has_property_on_prototype("))
            for body in hooks.values():
                self.assertEqual(1, body.count(NAMED))


if __name__ == "__main__":
    unittest.main()
