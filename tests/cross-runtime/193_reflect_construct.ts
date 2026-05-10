// Cross-runtime: Reflect.construct and apply
function Point(this: any, x: number, y: number) {
  this.x = x;
  this.y = y;
}

const p = Reflect.construct(Point, [10, 20]);
console.log("x=" + p.x);
console.log("y=" + p.y);

function sum(a: number, b: number) {
  return a + b;
}

const result = Reflect.apply(sum, null, [5, 3]);
console.log("apply=" + result);

const max = Reflect.apply(Math.max, null, [1, 5, 3, 9, 2]);
console.log("max=" + max);
