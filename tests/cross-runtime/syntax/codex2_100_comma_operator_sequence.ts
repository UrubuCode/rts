// Cross-runtime: comma expressions evaluate every operand and yield the last.
const seen: number[] = [];
const mark = (n: number) => { seen.push(n); return n * 10; };
const value = (mark(1), mark(2), mark(3));
console.log(value, seen.join(","));
let x = 0;
x = (x += 1, x += 2, x += 3);
console.log(x);

