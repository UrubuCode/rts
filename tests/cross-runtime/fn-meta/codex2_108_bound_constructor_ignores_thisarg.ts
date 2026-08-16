// Cross-runtime: constructing a bound function ignores the bound receiver.
function Point(this: any, x: number, y: number) {
  this.x = x;
  this.y = y;
}
const fake: any = {};
const Bound: any = Point.bind(fake, 4);
const p = new Bound(7);
console.log(p.x, p.y, fake.x);
console.log(p instanceof Point, p instanceof Bound);

