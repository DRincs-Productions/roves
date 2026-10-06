[Exposed=Window]
interface ConstructibleCounter {
  constructor(unsigned long start, optional DOMString label = "counter");
  readonly attribute unsigned long value;
  readonly attribute DOMString label;
  unsigned long increment();
};
