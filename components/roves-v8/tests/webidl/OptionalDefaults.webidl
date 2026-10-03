[Exposed=Window] interface OptionalDefaults {
  long booleanDefault(optional boolean value = false);
  long integerDefault(optional long value = 0);
  unsigned long unsignedDefault(optional unsigned long value = 6);
  float finiteFloatDefault(optional float value = 2.5);
  double finiteDefault(optional double value = 1.5);
  unrestricted float unrestrictedFloatDefault(optional unrestricted float value = 2.5);
  unrestricted double infinityDefault(optional unrestricted double value = Infinity);
  long nullableDefault(optional long? value = null);
  long nullableValueDefault(optional long? value = 3);
};
