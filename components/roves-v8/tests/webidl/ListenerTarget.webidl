[Exposed=Window]
callback interface ProbeListener {
  undefined handleEvent(any event);
};

dictionary ListenOptions {
  boolean once = false;
};

[Exposed=Window]
interface ListenerTarget {
  constructor();
  undefined listen(ProbeListener? listener, optional (ListenOptions or boolean) options = {});
  unsigned long dispatch(any detail);
  DOMString kind((long or DOMString or ListenerTarget) value);
};
