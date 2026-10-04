[Exposed=Window] interface OptionalNullableStringDefaults {
  long dom(optional DOMString? value = null);
  long usv(optional USVString? value = null);
  long bytes(optional ByteString? value = null);
};
