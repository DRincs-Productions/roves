[Exposed=Window]
interface OptionalOperations {
    long? classify(optional long? value);
    long fallback(optional long value);
};
