[Exposed=Window]
interface JsonBase {
  readonly attribute unsigned long id;
  [Unscopable] readonly attribute boolean flag;
  [Default] object toJSON();
};
