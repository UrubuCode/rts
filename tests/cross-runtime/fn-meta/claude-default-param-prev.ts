function f(a: number, b: number = a * 2, c: number = a + b): number {
  return a + b + c;
}
console.log(f(1));
console.log(f(1, 10));
console.log(f(1, 10, 100));
console.log(f(5));

function g(x: number, y: number = x + 1): string {
  return [x, y].join(",");
}
console.log(g(3));
console.log(g(3, 9));