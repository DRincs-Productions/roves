// Named getter, setter and deleter with identifiers (like Storage), and an indexed setter
// (like HTMLOptionsCollection).
[Exposed=Window]
interface StorageProbe {
  readonly attribute unsigned long length;
  getter DOMString? getItem(DOMString name);
  setter undefined setItem(DOMString name, DOMString value);
  deleter undefined removeItem(DOMString name);
  getter DOMString? (unsigned long index);
  setter undefined (unsigned long index, DOMString? value);
};
