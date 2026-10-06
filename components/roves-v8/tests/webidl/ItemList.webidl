[Exposed=Window]
interface ItemList {
  getter DOMString? item(unsigned long index);
  readonly attribute unsigned long length;
  iterable<DOMString?>;
};
