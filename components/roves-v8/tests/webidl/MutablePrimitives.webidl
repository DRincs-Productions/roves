[Exposed=Window]
interface MutablePrimitives {
  attribute boolean enabled;
  attribute double ratio;
  attribute unsigned long count;
  attribute boolean? optionalEnabled;
  attribute double? optionalRatio;
  attribute unsigned long? optionalCount;
};
