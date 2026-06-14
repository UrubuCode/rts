function pure(x: number): number { return (x * 16807) % 2147483647; }
let acc = 0; let s = 123456789;
const t0 = Date.now();
for (let i = 0; i < 25000000; i++) { s = pure(s); acc = acc + (s % 7); }
console.log("ms=" + (Date.now() - t0) + " acc=" + acc);
