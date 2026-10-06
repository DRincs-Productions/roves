[Exposed=Window]
interface StyleProbe {
  constructor();
  attribute (DOMString or GradientProbe) fillStyle;
  attribute (unsigned long or DOMString)? lineDash;
  readonly attribute Uint8Array pixels;
};
