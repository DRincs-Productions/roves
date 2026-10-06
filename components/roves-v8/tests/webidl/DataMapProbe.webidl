// Anonymous named getter/setter/deleter with [LegacyOverrideBuiltIns] (like DOMStringMap).
[Exposed=Window, LegacyOverrideBuiltIns]
interface DataMapProbe {
  getter DOMString (DOMString name);
  setter undefined (DOMString name, DOMString value);
  deleter undefined (DOMString name);
};
