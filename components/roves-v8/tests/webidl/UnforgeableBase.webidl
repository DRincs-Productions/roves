[Exposed=Window]
interface UnforgeableBase {
  [LegacyUnforgeable] readonly attribute boolean trusted;
  readonly attribute boolean ordinary;
};
