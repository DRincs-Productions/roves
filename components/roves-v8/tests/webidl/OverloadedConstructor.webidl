// An overloaded constructor (like Path2D and ImageData): `new` picks the overload with WebIDL's
// overload resolution algorithm.
[Exposed=Window]
interface Shape {
  constructor();
  constructor(DOMString name);
  constructor(unsigned long sides, optional boolean filled = false);
  readonly attribute DOMString description;
};
