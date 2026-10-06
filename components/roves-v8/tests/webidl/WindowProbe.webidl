// A [Global] interface (like Window): the realm's global object is an instance of it.
[Global=Window, Exposed=Window]
interface WindowProbe : GlobalScopeProbe {
  readonly attribute DOMString title;
  attribute unsigned long counter;
  DOMString greet(DOMString who);
  [BinaryName="Self_"] readonly attribute WindowProbe self;
  [Replaceable] readonly attribute unsigned long length;
};
