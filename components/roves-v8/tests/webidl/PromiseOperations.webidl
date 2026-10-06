[Exposed=Window]
interface PromiseOperations {
  Promise<unsigned long> twice(unsigned long value);
  Promise<undefined> later();
  undefined settle(boolean succeed);
  Promise<any> wrap(Promise<any> input);
  // Static and overloaded promise operations reject on conversion errors too.
  static Promise<unsigned long> half(unsigned long value);
  Promise<unsigned long> count(DOMString text);
  Promise<unsigned long> count(sequence<long> values);
};
