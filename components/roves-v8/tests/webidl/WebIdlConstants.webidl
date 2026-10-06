[Exposed=Window]
interface WebIdlConstants {
  const unsigned short NONE = 0;
  const unsigned short CAPTURING_PHASE = 1;
  const unsigned long LARGEST = 4294967295;
  const long long NEGATIVE = -5;
  const unrestricted double POSITIVE_INFINITY = Infinity;
  const unrestricted float NOT_A_NUMBER = NaN;
  const boolean ENABLED = true;
  readonly attribute unsigned short phase;
};
