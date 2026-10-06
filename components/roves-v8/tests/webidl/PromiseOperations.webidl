[Exposed=Window]
interface PromiseOperations {
  Promise<unsigned long> twice(unsigned long value);
  Promise<undefined> later();
  undefined settle(boolean succeed);
  Promise<any> wrap(Promise<any> input);
};
