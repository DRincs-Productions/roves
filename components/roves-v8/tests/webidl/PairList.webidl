// A pair iterable (like FormData, Headers and URLSearchParams).
[Exposed=Window]
interface PairList {
  iterable<DOMString, unsigned long>;
  undefined add(DOMString key, unsigned long value);
};
