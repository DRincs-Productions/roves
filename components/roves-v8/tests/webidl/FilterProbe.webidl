// A callback interface with constants (like NodeFilter): only a legacy callback interface
// object holding the constants is exposed.
[Exposed=Window]
callback interface FilterProbe {
  const unsigned short FILTER_ACCEPT = 1;
  const unsigned short FILTER_SKIP = 3;
  unsigned short acceptNode(any node);
};
