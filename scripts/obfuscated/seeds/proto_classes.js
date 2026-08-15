// Classes, prototypes, getters, static, super, instanceof.
const out = [];
class Shape {
  constructor(name) { this.name = name; }
  get label() { return "shape:" + this.name; }
  static kind() { return "Shape"; }
  describe() { return this.label; }
}
class Circle extends Shape {
  constructor(r) { super("circle"); this.r = r; }
  get label() { return super.label + ":" + this.r; }
  static kind() { return "C<" + super.kind() + ">"; }
}
const k = new Circle(3);
out.push(k.describe());
out.push(Circle.kind());
out.push(k instanceof Shape, k instanceof Circle);
out.push(Object.getPrototypeOf(Circle.prototype) === Shape.prototype);

const proto = { greet() { return "hi " + this.who; } };
const obj = Object.create(proto);
obj.who = "you";
out.push(obj.greet());
out.push(Object.keys(obj).join(","));

function Old(v) { this.v = v; }
Old.prototype.twice = function () { return this.v * 2; };
out.push(new Old(21).twice());

console.log(out.join("|"));
