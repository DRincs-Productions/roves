[Exposed=Window]
interface OverloadedOperations {
  DOMString describe();
  DOMString describe(unsigned long count, optional boolean loud = false);
  unsigned long measure(DOMString text);
  unsigned long measure(DOMString text, unsigned long scale, unsigned long offset);
  [Throws] undefined reset();
  [Throws] undefined reset(boolean hard, boolean deep);
};
