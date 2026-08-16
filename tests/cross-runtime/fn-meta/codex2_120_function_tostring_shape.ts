// Cross-runtime: Function.prototype.toString preserves key source distinctions.
function named(a: number) { return a + 1; }
const arrow = (x: number) => x * 2;
const method = { run(x: number) { return x; } }.run;
console.log(Function.prototype.toString.call(named).includes("named"));
console.log(arrow.toString().includes("=>"));
console.log(method.toString().startsWith("run"));

