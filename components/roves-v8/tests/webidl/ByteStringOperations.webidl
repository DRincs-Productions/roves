[Exposed=Window]
interface ByteStringOperations {
    DOMString echo(ByteString value);
    DOMString? nullableEcho(ByteString? value);
    DOMString? optionalEcho(optional ByteString? value);
};
