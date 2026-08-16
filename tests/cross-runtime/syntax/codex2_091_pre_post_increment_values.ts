// Cross-runtime: prefix and postfix increments expose old and new values.
let x = 3;
const a = x++;
const b = ++x;
const c = x--;
const d = --x;
console.log(a, b, c, d, x);

