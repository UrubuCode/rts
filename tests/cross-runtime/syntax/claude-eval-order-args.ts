let log: number[] = [];
function t(x: number): number { log.push(x); return x; }
function f(a: number, b: number, c: number): number { return a * 100 + b * 10 + c; }
let r = f(t(1), t(2), t(3));
console.log(log.join(","));
console.log(r);
let g = (x: number, y: number) => x - y;
let r2 = g(t(7), t(4));
console.log(log.join(","));
console.log(r2);