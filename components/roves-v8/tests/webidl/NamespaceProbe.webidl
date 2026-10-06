// A WebIDL namespace (like `console` and `CSS`): a plain object holding operations,
// attributes and constants, with no constructor or prototype.
[Exposed=Window, ClassString="Probe"]
namespace probe {
  const unsigned short MODE = 2;
  readonly attribute DOMString version;
  [Pref="dom_probe_enabled"] readonly attribute boolean gated;
  [Throws] DOMString shout(DOMString text);
  unsigned long count(any... items);
  boolean supports(DOMString property, DOMString value);
  boolean supports(DOMString conditionText);
  DOMString kind(long value);
  DOMString kind(sequence<long> value);
};
