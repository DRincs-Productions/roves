[Exposed=Window]
interface CeReactive {
  [CEReactions] attribute DOMString title;
  [CEReactions] undefined touch();
  [CEReactions, Throws] undefined fail();
  readonly attribute unsigned long observedDepth;
};
