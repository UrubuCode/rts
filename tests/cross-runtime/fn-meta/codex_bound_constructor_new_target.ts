// Cross-runtime: bound functions used as constructors.
function Point(this: any, x: number, y: number) {
  this.x = x;
  this.y = y;
}
Point.prototype.sum = function () { return this.x + this.y; };

const Bound: any = Point.bind(null, 2);
const p = new Bound(5);
console.log(p.x + ":" + p.y + ":" + p.sum());
console.log(p instanceof Point);
console.log(p instanceof Bound);
