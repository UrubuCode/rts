// Cross-runtime: a default parameter may call a prior callback parameter.
function compute(fn: (n: number) => number, value = fn(4)) {
  return value;
}
console.log(compute((n) => n * 3));
console.log(compute((n) => n * 3, 99));

