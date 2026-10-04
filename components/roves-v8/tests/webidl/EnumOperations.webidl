enum Direction { "left", "right" };

[Exposed=Window] interface EnumOperations {
  USVString echo(Direction value);
  USVString optionalValue(optional Direction value = "right");
};
