// Cross-runtime: arguments object edge cases (only in non-arrow functions).

// basic access
function sum() {
  let total = 0;
  for (let i = 0; i < arguments.length; i++) total += arguments[i];
  return total;
}
console.log(sum(1, 2, 3, 4, 5));  // 15

// arguments is array-like, not an Array
function argsType() {
  console.log(Array.isArray(arguments));  // false
  console.log(typeof arguments);          // object
  console.log(arguments.length);
  // convert to real array
  return Array.from(arguments);
}
const arr = argsType(10, 20, 30);
console.log(arr.join(","));  // 10,20,30

// arguments reflects parameter changes (sloppy mode)
function mutate(a: number, b: number) {
  a = 99;
  return [a, arguments[0], b, arguments[1]];
}
const r = mutate(1, 2);
console.log(r.join(","));  // 99,99,2,2 (sloppy: arguments mirrors params)

// arguments in nested function (inner arrow captures outer arguments)
function outer(x: number) {
  const inner = () => arguments[0];  // arrow captures outer arguments
  return inner();
}
console.log(outer(42));  // 42

// rest params vs arguments
function withRest(first: number, ...rest: number[]) {
  return [first, rest.length, arguments.length];
}
const wr = withRest(1, 2, 3, 4);
console.log(wr.join(","));  // 1,3,4

// arguments spread pattern (before rest params existed)
function applyOld(fn: Function) {
  const args = Array.prototype.slice.call(arguments, 1);
  return fn.apply(null, args);
}
console.log(applyOld(Math.max, 3, 1, 4, 1, 5));  // 5
