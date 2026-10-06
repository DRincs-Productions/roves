// Interface-level [LegacyUnforgeable] (like Location): every member is a non-configurable own
// property of each instance.
[Exposed=Window, LegacyUnforgeable]
interface LocationProbe {
  [Throws, CrossOriginWritable] stringifier attribute USVString href;
  readonly attribute USVString origin;
  [Throws] undefined assign(USVString url);
};
