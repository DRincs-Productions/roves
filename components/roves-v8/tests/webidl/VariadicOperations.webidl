[Exposed=Window]
interface VariadicOperations {
  unsigned long count(DOMString... tokens);
  DOMString join(DOMString separator, DOMString... parts);
  long sum(long... values);
};
