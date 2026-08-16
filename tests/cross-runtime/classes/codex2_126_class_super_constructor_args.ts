// Cross-runtime: derived constructors transform arguments before super initialization.
class Pair {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}
class Shifted extends Pair {
  extra: number;
  constructor(x: number, y: number) {
    super(x + 1, y + 2);
    this.extra = this.x + this.y;
  }
}
const p = new Shifted(3, 4);
console.log(p.x, p.y, p.extra);
