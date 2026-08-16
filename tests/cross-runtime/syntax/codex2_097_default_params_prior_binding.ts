// Cross-runtime: default parameters can reference earlier parameter bindings.
function f(a = 2, b = a * 3, c = b + a) {
  return [a, b, c].join(",");
}
console.log(f());
console.log(f(4));
console.log(f(4, undefined, 20));

