[LegacyTreatNonObjectAsNull]
callback EventHandlerNonNull = any (any event);
typedef EventHandlerNonNull? EventHandler;

[Exposed=Window]
interface HandlerHost {
  attribute EventHandler onping;
  attribute any data;
  readonly attribute object? shape;
  any fire(any detail);
};
