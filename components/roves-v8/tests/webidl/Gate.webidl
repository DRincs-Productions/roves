// Static attributes (like Notification.permission) and [LegacyLenientSetter] (like
// DocumentOrShadowRoot.fullscreenElement).
[Exposed=Window]
interface Gate {
  static readonly attribute unsigned long limit;
  [LegacyLenientSetter] readonly attribute boolean open;
};
