// Cross-runtime: destructuring with default values.

// object defaults
const { a = 10, b = 20, c = 30 } = { a: 1, c: 3 };
console.log(a, b, c);

// nested defaults
const { x: { y = 5 } = {} } = { x: {} };
console.log(y);

// array defaults
const [p = 9, q = 8, r = 7] = [1, 2];
console.log(p, q, r);

// function param defaults via destructuring
function greet({ name = "world", times = 1 }: { name?: string; times?: number }) {
  for (let i = 0; i < times; i++) console.log("hello " + name);
}
greet({});
greet({ name: "rts", times: 2 });

// rename + default
const { foo: bar = 42 } = {};
console.log(bar);
