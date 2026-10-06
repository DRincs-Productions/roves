[Exposed=Window, LegacyUnenumerableNamedProperties]
interface NamedCollection {
  getter DOMString? item(unsigned long index);
  getter DOMString? namedItem(DOMString name);
  readonly attribute unsigned long length;
};
