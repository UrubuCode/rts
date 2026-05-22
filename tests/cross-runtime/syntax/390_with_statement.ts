// Cross-runtime: with statement (deprecated, sloppy mode only).
// Note: forbidden in strict mode. This file must not have "use strict".

// with adds object to scope chain — rarely used, but valid JS
const rect = { width: 10, height: 5, color: "red" };
let area: number;
let desc: string;

// @ts-ignore — with is valid JS but TypeScript disallows it
with (rect) {
  area = width * height;   // resolves width/height from rect
  desc = color + " rect";
}
console.log(area);  // 50
console.log(desc);  // red rect

// with Math (historical pattern for math-heavy code)
let result: number;
// @ts-ignore
with (Math) {
  result = round(sqrt(pow(3, 2) + pow(4, 2)));  // 5
}
console.log(result);  // 5

// with on computed object
function compute(vals: { a: number; b: number; c: number }): number {
  let out = 0;
  // @ts-ignore
  with (vals) {
    out = a * b + c;
  }
  return out;
}
console.log(compute({ a: 3, b: 4, c: 2 }));  // 14

// with and var declaration (scoping subtlety)
// @ts-ignore
with ({ x: 10 }) {
  var captured = x;  // var hoists to function scope, not with block
}
console.log(captured);  // 10
