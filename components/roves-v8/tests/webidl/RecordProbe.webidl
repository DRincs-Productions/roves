// record<K, V> (like Headers' HeadersInit and URLSearchParams' init): arguments, results and
// a union member chosen for non-iterable objects.
[Exposed=Window]
interface RecordProbe {
  DOMString describe(record<DOMString, long> counts);
  record<USVString, unsigned long> tally(sequence<USVString> words);
  DOMString pick((sequence<sequence<DOMString>> or record<DOMString, DOMString> or DOMString) init);
};
