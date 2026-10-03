[Exposed=Window] interface OptionalStringDefaults {
  DOMString dom(optional DOMString value = "line\nquote");
  USVString usv(optional USVString value = "rocket 🚀");
  DOMString unicode(optional DOMString value = "rocket 🚀");
  DOMString? nullable(optional DOMString? value = "seed");
  DOMString bytes(optional ByteString value = "abc");
};
