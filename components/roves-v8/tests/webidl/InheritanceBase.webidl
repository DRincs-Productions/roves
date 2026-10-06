[Exposed=Window]
interface InheritanceBase {
  readonly attribute unsigned long depth;
  attribute boolean flagged;
  boolean isBase();
};
