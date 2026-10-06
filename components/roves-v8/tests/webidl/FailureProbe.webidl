// [ExceptionClass] (like DOMException), a [SecureContext] constructor (like ClipboardItem),
// [Replaceable] (like self.origin) and an [EnforceRange] attribute (like OffscreenCanvas.width).
[Exposed=Window, ExceptionClass]
interface FailureProbe {
  [SecureContext] constructor(optional DOMString message = "");
  readonly attribute DOMString message;
  [Replaceable] readonly attribute DOMString origin;
  attribute [EnforceRange] unsigned long long width;
};
