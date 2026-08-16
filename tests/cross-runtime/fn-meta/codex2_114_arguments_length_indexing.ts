// Cross-runtime: arguments exposes indexed values and call arity.
function inspect(a: any) {
  return [a, arguments[1], arguments.length, Object.keys(arguments).join(",")].join("|");
}
console.log(inspect("a", "b", "c"));
console.log(inspect());

