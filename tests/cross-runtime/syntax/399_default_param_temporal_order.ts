// Cross-runtime: default parameter evaluation order and arguments visibility.
function f(a: number, b = a + 1, c = b + arguments.length) {
  return [a, b, c].join(",");
}

function g(a = 1, b = () => a, c = (a = 5)) {
  return b() + ":" + c + ":" + a;
}

console.log(f(2));
console.log(f(2, 10));
console.log(f(2, undefined, 20));
console.log(g());
console.log(g(9));
