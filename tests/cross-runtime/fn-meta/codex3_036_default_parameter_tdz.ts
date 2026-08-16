// Cross-runtime: a default parameter cannot read a later parameter before initialization.
function f(a = b, b = 2) { return a + b; }
let first = false;
try { f(); } catch (e) { first = e instanceof ReferenceError; }
console.log(first);
console.log(f(3), f(3, 4));

