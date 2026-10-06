callback NumberCallback = unsigned long (unsigned long value);

[Exposed=Window]
interface CallbackOperations {
  unsigned long apply(NumberCallback callback, unsigned long value);
  any echo(any value);
  boolean isObject(object value);
  unsigned long countCalls(NumberCallback? callback);
  attribute unsigned long total;
};
