[Exposed=Window]
interface NullableOperations {
    boolean? flag(boolean? value);
    long? count(long? value);
    DOMString? label(DOMString? value);
    USVString? name(USVString? value);
    long? mix(long? value, long addend);
};
