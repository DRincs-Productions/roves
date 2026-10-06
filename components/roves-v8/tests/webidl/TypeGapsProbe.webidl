enum ProbeState { "running", "suspended", "closed" };

[Exposed=Window]
callback interface ProbeFilter {
  unsigned short acceptNode(any node);
};

[Exposed=Window]
interface TypeGapsProbe {
  attribute ProbeState state;
  attribute ProbeState? maybeState;
  readonly attribute ByteString raw;
  ProbeState next();
  (DOMString or unsigned long)? pick(boolean asText);
  ByteString? header(boolean present);
  octet clamp([Clamp] octet value);
  long strict([EnforceRange] long value);
  unsigned long filtered(optional ProbeFilter? filter = null);
};
