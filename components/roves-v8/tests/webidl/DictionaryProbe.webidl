dictionary ProbeOptions {
  boolean deep = true;
};

dictionary BaseInit {
  boolean bubbles = false;
  boolean cancelable = false;
};

dictionary ProbeInit : BaseInit {
  required DOMString label;
  unsigned long count;
  sequence<long> values = [];
  ProbeOptions nested = {};
};

[Exposed=Window]
interface DictionaryProbe {
  constructor(DOMString type, optional BaseInit init = {});
  readonly attribute DOMString type;
  readonly attribute boolean bubbles;
  readonly attribute boolean cancelable;
  ProbeInit describe(ProbeInit init);
};
