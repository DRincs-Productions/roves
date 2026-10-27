"""Named-property visibility in the generated proxy handlers ([LegacyOverrideBuiltIns]).

Runs the vendored WebIDL parser and the real binding generator and checks the order of the
lookups in the generated get, getOwnPropertyDescriptor and hasOwn hooks, plus the compile-time
flag the generated ownKeys hook passes to proxyhandler:
https://webidl.spec.whatwg.org/#dfn-named-property-visibility

Two configurations are used: a small synthetic IDL (controls with and without the attribute),
and every real webidls/*.webidl file with the real Bindings.conf, parsed the way codegen/run.py
does, so the Document, HTMLDocument, XMLDocument, DOMStringMap and HTMLFormElement cases come
from the actual declarations. The negative case parses the same real files with the attribute
removed from Document.webidl.
"""

import os
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
from run import FILTER_PATTERN  # noqa: E402
from codegen import (  # noqa: E402
    CGDOMJSProxyHandler_get,
    CGDOMJSProxyHandler_getOwnPropertyDescriptor,
    CGDOMJSProxyHandler_hasOwn,
    CGDOMJSProxyHandler_ownPropertyKeys,
    hasLegacyOverrideBuiltIns,
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
CONFIG = "DOMInterfaces = {}\nEnums = {}\nDictionaries = {}\nUnions = {}\nSubCrates = {'script_webgpu': []}\n"
REAL_CONFIG = BINDINGS / "codegen" / "Bindings.conf"
WEBIDLS = BINDINGS / "webidls"
# The non-partial Document declaration carries the HTML partial's [LegacyOverrideBuiltIns]
# (the parser rejects it on partial interfaces); the negative case removes exactly this.
DOCUMENT_OVERRIDE = "[Exposed=Window, LegacyOverrideBuiltIns]\ninterface Document : Node {"
DOCUMENT_PLAIN = "[Exposed=Window]\ninterface Document : Node {"

NAMED = "NamedGetter("
INDEXED = "IndexedGetter("
SYNTHETIC_OVERRIDE = ("OverrideNamed", "OverrideIndexed")
SYNTHETIC_PLAIN = ("PlainNamed", "PlainIndexed")
# Real interfaces: Document carries the attribute, HTMLDocument and DOMStringMap declare it
# themselves, XMLDocument inherits it from Document. HTMLFormElement has no attribute.
REAL_OVERRIDE = ("Document", "HTMLDocument", "XMLDocument", "DOMStringMap")
REAL_PLAIN = ("HTMLFormElement",)

SCRATCH = tempfile.TemporaryDirectory()
CONFIGS = {}


def tearDownModule():
    SCRATCH.cleanup()


def synthetic_configuration():
    parser = WebIDL.Parser(SCRATCH.name)
    parser.parse(IDL, "NamedPropertyVisibility.webidl")
    config_file = Path(SCRATCH.name) / "Bindings.conf"
    config_file.write_text(CONFIG, encoding="utf-8")
    return Configuration(str(config_file), parser.finish())


def real_configuration(document_override):
    # Same file selection as codegen/run.py (minus the generated CSSStyleDeclaration partial).
    parser = WebIDL.Parser(SCRATCH.name)
    for path in sorted(WEBIDLS.glob("*.webidl")):
        contents = path.read_text(encoding="utf-8")
        filter_match = FILTER_PATTERN.search(contents)
        if filter_match and not os.environ.get(filter_match.group(1)):
            continue
        if path.name == "Document.webidl":
            if contents.count(DOCUMENT_OVERRIDE) != 1:
                raise AssertionError("Document.webidl no longer has the expected declaration")
            if not document_override:
                contents = contents.replace(DOCUMENT_OVERRIDE, DOCUMENT_PLAIN)
        parser.parse(contents, str(path))
    return Configuration(str(REAL_CONFIG), parser.finish())


def descriptor(name, kind="synthetic"):
    if kind not in CONFIGS:
        CONFIGS[kind] = {
            "synthetic": synthetic_configuration,
            "real": lambda: real_configuration(True),
            "negative": lambda: real_configuration(False),
        }[kind]()
    return CONFIGS[kind].getDescriptor(name)


def cases(synthetic, real):
    return [(name, "synthetic") for name in synthetic] + [(name, "real") for name in real]


OVERRIDE = cases(SYNTHETIC_OVERRIDE, REAL_OVERRIDE)
PLAIN = cases(SYNTHETIC_PLAIN, REAL_PLAIN)


def bodies(name, kind):
    d = descriptor(name, kind)
    return {
        "get": CGDOMJSProxyHandler_get(d).getBody(),
        "getOwnPropertyDescriptor": CGDOMJSProxyHandler_getOwnPropertyDescriptor(d).getBody(),
        "hasOwn": CGDOMJSProxyHandler_hasOwn(d).getBody(),
    }


def own_keys_body(name, kind):
    return CGDOMJSProxyHandler_ownPropertyKeys(descriptor(name, kind)).definition_body().define()


class NamedPropertyVisibilityTests(unittest.TestCase):
    def assertBefore(self, body, first, second):
        self.assertIn(first, body)
        self.assertIn(second, body)
        self.assertLess(body.index(first), body.index(second), f"{first!r} must come before {second!r}")

    def assertOverrideHooks(self, name, kind):
        hooks = bodies(name, kind)
        self.assertBefore(hooks["get"], "get_expando_object(", NAMED)
        self.assertBefore(hooks["get"], NAMED, "get_property_on_prototype(")
        for hook, expando in (("getOwnPropertyDescriptor", "JS_GetPropertyDescriptorById("),
                              ("hasOwn", "JS_HasPropertyById(")):
            body = hooks[hook]
            self.assertNotIn("has_property_on_prototype", body, f"{name}.{hook}")
            self.assertBefore(body, "get_expando_object(", NAMED)
            self.assertBefore(body, expando, NAMED)

    def assertPlainHooks(self, name, kind):
        hooks = bodies(name, kind)
        self.assertBefore(hooks["get"], "get_expando_object(", "get_property_on_prototype(")
        self.assertBefore(hooks["get"], "get_property_on_prototype(", NAMED)
        for hook in ("getOwnPropertyDescriptor", "hasOwn"):
            self.assertBefore(hooks[hook], "get_expando_object(", "has_property_on_prototype(")
            self.assertBefore(hooks[hook], "has_property_on_prototype(", NAMED)

    def assertOwnKeysFlag(self, name, kind, flag):
        body = own_keys_body(name, kind)
        self.assertEqual(1, body.count("legacy_override_builtins:"), name)
        self.assertIn(f"legacy_override_builtins: {flag},", body, name)
        self.assertIn("SupportedPropertyNames(", body, name)

    def test_extended_attribute_is_read_from_the_idl(self):
        for name, kind in OVERRIDE:
            d = descriptor(name, kind)
            self.assertTrue(d.proxy and d.supportsNamedProperties(), name)
            self.assertTrue(hasLegacyOverrideBuiltIns(d), name)
        for name, kind in PLAIN:
            d = descriptor(name, kind)
            self.assertTrue(d.proxy and d.supportsNamedProperties(), name)
            self.assertFalse(hasLegacyOverrideBuiltIns(d), name)

    def test_real_document_declares_the_attribute(self):
        # The reflector of every document is a Document (or XMLDocument): the attribute must be
        # on Document itself, not only on the HTMLDocument interface.
        attribute = "LegacyOverrideBuiltIns"
        self.assertTrue(descriptor("Document", "real").interface.getExtendedAttribute(attribute))
        self.assertTrue(descriptor("HTMLDocument", "real").interface.getExtendedAttribute(attribute))
        xml = descriptor("XMLDocument", "real").interface
        self.assertFalse(xml.getExtendedAttribute(attribute))
        self.assertEqual("Document", xml.parent.identifier.name)

    def test_override_hooks_read_named_before_the_prototype(self):
        for name, kind in OVERRIDE:
            self.assertOverrideHooks(name, kind)

    def test_plain_hooks_let_the_prototype_hide_named(self):
        for name, kind in PLAIN:
            self.assertPlainHooks(name, kind)

    def test_indexed_properties_stay_first_and_exclude_named(self):
        for name in ("OverrideIndexed", "PlainIndexed"):
            for hook, body in bodies(name, "synthetic").items():
                self.assertBefore(body, INDEXED, NAMED)
                self.assertIn("index.is_none() && (id.is_string() || id.is_int())", body, f"{name}.{hook}")
            # get forwards a missing index neither to the expando nor to the named getter.
            self.assertBefore(bodies(name, "synthetic")["get"], INDEXED, "get_expando_object(")

    def test_plain_output_matches_the_previous_shape(self):
        # Without the attribute the named lookup stays after the prototype check, once per hook.
        for name in SYNTHETIC_PLAIN:
            hooks = bodies(name, "synthetic")
            self.assertEqual(1, hooks["get"].count("get_property_on_prototype("))
            self.assertEqual(1, hooks["getOwnPropertyDescriptor"].count("has_property_on_prototype("))
            self.assertEqual(1, hooks["hasOwn"].count("has_property_on_prototype("))
            for body in hooks.values():
                self.assertEqual(1, body.count(NAMED))

    def test_own_keys_config_carries_the_idl_attribute(self):
        # ownKeys filters hidden names in proxyhandler; the generator only forwards the flag.
        for names, flag in ((OVERRIDE, "true"), (PLAIN, "false")):
            for name, kind in names:
                self.assertOwnKeysFlag(name, kind, flag)

    def test_without_the_document_attribute_document_hooks_are_wrong(self):
        # Regression guard: the real Document.webidl without the attribute generates the
        # prototype-first order and a false ownKeys flag for Document and XMLDocument, although
        # HTMLDocument (not the reflector of any document) keeps its own attribute.
        for name in ("Document", "XMLDocument"):
            self.assertFalse(hasLegacyOverrideBuiltIns(descriptor(name, "negative")), name)
            self.assertPlainHooks(name, "negative")
            self.assertOwnKeysFlag(name, "negative", "false")
        self.assertOverrideHooks("HTMLDocument", "negative")
        self.assertOwnKeysFlag("HTMLDocument", "negative", "true")


if __name__ == "__main__":
    unittest.main()
