[Exposed=Window]
interface MutablePrimitives {
  attribute boolean enabled;
  attribute double ratio;
  attribute unsigned long count;
  attribute boolean? optionalEnabled;
  attribute double? optionalRatio;
  attribute unsigned long? optionalCount;
  undefined ping();
  boolean isEnabled();
  double currentRatio();
  unsigned long currentCount();
  boolean accepts(boolean value);
  DOMString currentLabel();
  DOMString? optionalLabel();
  USVString currentUsvLabel();
  USVString? optionalUsvLabel();
};
