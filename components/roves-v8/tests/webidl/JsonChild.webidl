[Exposed=Window]
interface JsonChild : JsonBase {
  readonly attribute DOMString name;
  readonly attribute any hidden;
  [Unscopable] undefined before();
  [NewObject] JsonChild clone();
  static unsigned long count(unsigned long base);
  [Default] object toJSON();
};
