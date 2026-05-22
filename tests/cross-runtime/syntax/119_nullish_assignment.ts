// Cross-runtime: nullish coalescing assignment (??=)
let a: number | null = null;
let b: number | undefined = undefined;
let c = 5;

a ??= 10;
b ??= 20;
c ??= 30;

console.log("a=" + a);
console.log("b=" + b);
console.log("c=" + c);

// Logical assignment operators
let x = 0;
let y = 5;

x ||= 100;
y ||= 200;

console.log("x=" + x);
console.log("y=" + y);

let p = true;
let q = false;

p &&= false;
q &&= true;

console.log("p=" + p);
console.log("q=" + q);
