[Exposed=Window]
interface ThrowingOperations {
  [Throws] constructor(boolean allow);
  [Throws] unsigned long parse(DOMString text);
  [Throws] undefined reset(boolean fail);
};
