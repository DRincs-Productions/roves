[Exposed=Window]
interface SequenceOperations {
  unsigned long sum(sequence<unsigned long> values);
  sequence<DOMString> split(DOMString text);
  sequence<sequence<long>> grid(unsigned long size);
  sequence<long?> withHoles(sequence<long?> values);
  sequence<SequenceOperations> selves(sequence<SequenceOperations> items);
  unsigned long countFlags(optional sequence<boolean> flags = []);
  sequence<any> echoAll(sequence<any> values);
};
