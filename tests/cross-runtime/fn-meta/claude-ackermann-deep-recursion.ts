function ackermann(m: number, n: number): number {
  if (m === 0) return n + 1;
  if (n === 0) return ackermann(m - 1, 1);
  return ackermann(m - 1, ackermann(m, n - 1));
}
console.log(ackermann(0, 0));
console.log(ackermann(1, 1));
console.log(ackermann(2, 3));
console.log(ackermann(3, 3));