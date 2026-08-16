// Cross-runtime: destructuring assignment writes existing bindings in order.
let a = 0;
let b = 0;
let rest: number[] = [];
[a, b, ...rest] = [1, 2, 3, 4];
console.log(a, b, rest.join(","));
({ a, b } = { a: 7, b: 8 });
console.log(a, b);

