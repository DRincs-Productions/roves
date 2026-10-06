enum ProbeFillRule { "nonzero", "evenodd" };

[Exposed=Window]
interface DrawProbe {
  constructor();
  DOMString fill(optional ProbeFillRule rule = "nonzero");
  DOMString fill(PathProbe path, optional ProbeFillRule rule = "nonzero");
  DOMString measure(DOMString text);
  DOMString measure(unsigned long count);
  DOMString draw(sequence<long> points);
  DOMString draw(DOMString label);
};
