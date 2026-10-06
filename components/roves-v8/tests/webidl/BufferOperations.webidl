typedef (ArrayBufferView or ArrayBuffer) ProbeBufferSource;

[Exposed=Window]
interface BufferOperations {
  unsigned long sum(ArrayBufferView data);
  undefined fill(Uint8Array target, octet value);
  ArrayBuffer copy(ProbeBufferSource source);
  Float32Array halves(unsigned long count);
};
