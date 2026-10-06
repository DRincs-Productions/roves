// Mutable interface-typed attributes (like TreeWalker.currentNode and HTMLTableElement.caption):
// the setter accepts only instances of the interface (or null where nullable).
[Exposed=Window]
interface Holder {
  readonly attribute unsigned long id;
  attribute Holder? next;
  [SetterThrows] attribute Holder current;
};
