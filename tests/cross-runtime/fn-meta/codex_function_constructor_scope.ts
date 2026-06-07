// Cross-runtime: Function constructor uses global scope, not local scope.
const x = 10;
function make() {
  const x = 20;
  return Function("return typeof x === 'undefined' ? 'missing' : x");
}

console.log(make()());
console.log(Function("a", "b", "return a + b")(2, 3));
