// [LegacyFactoryFunction] (like Image, Audio and Option): a constructor function on the global
// whose `prototype` is the interface prototype object.
[Exposed=Window,
 LegacyFactoryFunction=Picture(optional unsigned long width, optional unsigned long height),
 LegacyFactoryFunction=Snapshot(DOMString name)]
interface FactoryProbe {
  readonly attribute DOMString label;
};
