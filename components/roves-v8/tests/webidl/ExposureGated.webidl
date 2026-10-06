[Exposed=Window, Pref="dom_gated_enabled"]
interface ExposureGated {
  readonly attribute boolean always;
  [Pref="dom_gated_extra_enabled"] readonly attribute boolean extra;
  [SecureContext] boolean secret();
};
