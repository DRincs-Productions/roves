[Exposed=Window]
interface AttributeProbe {
  [SetterThrows] attribute DOMString? label;
  [GetterThrows] readonly attribute unsigned long checked;
  [Throws] attribute boolean flag;
  attribute byte small;
  attribute long long big;
  attribute unrestricted float ratio;
  [PutForwards=value] readonly attribute TokenProbe tokens;
};
