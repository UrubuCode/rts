let x = (1, 2, 3);
console.log(x);
let log: number[] = [];
function s(v: number): number { log.push(v); return v; }
let y = (s(10), s(20), s(30));
console.log(y);
console.log(log.join(","));
let i = 0, j = 10;
for (i = 0, j = 10; i < 3; i++, j--) {
  console.log(i + ":" + j);
}
let r = (s(1), s(2)) + (s(3), s(4));
console.log(r);
console.log(log.join(","));