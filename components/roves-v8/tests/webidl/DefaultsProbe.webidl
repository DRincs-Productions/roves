enum LatencyCategory { "interactive", "playback" };

dictionary LatencyOptions {
  (LatencyCategory or double) latencyHint = "interactive";
  any detail = null;
};

[Exposed=Window, LegacyWindowAlias=webkitDefaultsProbe, Func="probe_enabled", Serializable]
interface DefaultsProbe {
  DOMString kind(optional (boolean or LatencyOptions) options = false);
  any context(DOMString id, optional any options = null);
  DOMString latency(optional LatencyOptions options = {});
};
