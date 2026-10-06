[LegacyTreatNonObjectAsNull]
callback ProbeHandler = any (any event);
typedef ProbeHandler? ProbeEventHandler;

[Exposed=Window]
interface LenientProbe {
  [HTMLConstructor] constructor();
  [LegacyLenientThis] attribute ProbeEventHandler onmouseenter;
  attribute ProbeEventHandler onclick;
};
