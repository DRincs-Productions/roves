[Exposed=Window]
interface ConstructibleChild : ConstructibleCounter {
  constructor();
  readonly attribute boolean child;
};
