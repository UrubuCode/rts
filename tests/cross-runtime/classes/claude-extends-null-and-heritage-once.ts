// Cross-runtime: `extends null` gives a null prototype chain and an
// un-constructible class; the heritage expression is evaluated exactly once,
// at class-definition time; extending a bound function still works.
class NullProto extends null {}

console.log("null-proto-chain=" + (Object.getPrototypeOf(NullProto.prototype) === null));
console.log("null-ctor-proto=" + (Object.getPrototypeOf(NullProto) === Function.prototype));
console.log("null-proto-names=" + Object.getOwnPropertyNames(NullProto.prototype).join(","));

try {
  new NullProto();
  console.log("new-null=no-throw");
} catch (e: any) {
  console.log("new-null=" + e.constructor.name);
  console.log("new-null-is-type=" + (e instanceof TypeError));
}

// An object created against that prototype has no Object.prototype methods.
const bare: any = Object.create(NullProto.prototype);
bare.v = 1;
console.log("bare-has-tostring=" + ("toString" in bare));
console.log("bare-v=" + bare.v);
console.log("bare-tostring=" + Object.prototype.toString.call(bare));

// The heritage expression runs once per class definition, not per instance.
let heritageCalls = 0;
function heritage() {
  heritageCalls = heritageCalls + 1;
  return Base;
}
class Base {
  tag: string = "base";
  ping(): string {
    return "ping";
  }
}
class Sub extends heritage() {}
console.log("heritage-after-define=" + heritageCalls);
new Sub();
new Sub();
console.log("heritage-after-instances=" + heritageCalls);
console.log("sub-ping=" + new Sub().ping());

// It also runs before any computed key in the same class body.
const order: string[] = [];
function h2() {
  order.push("heritage");
  return Base;
}
class Ordered extends h2() {
  [(order.push("key"), "m")](): string {
    return "m";
  }
  static {
    order.push("static-block");
  }
}
console.log("order=" + order.join("|"));
console.log("ordered-m=" + (new Ordered() as any).m());

// A bound function has NO own "prototype", so it cannot be extended even
// though it is a constructor.
function Point(this: any, x: number, y: number) {
  this.x = x;
  this.y = y;
}
const BoundPoint: any = (Point as any).bind(null, 10);
console.log("bound-has-prototype=" + ("prototype" in BoundPoint));
console.log("bound-is-ctor=" + (Reflect.construct(BoundPoint, [20]) instanceof Point));
try {
  class BoundSub extends BoundPoint {}
  console.log("extends-bound=no-throw");
} catch (e: any) {
  console.log("extends-bound=" + e.constructor.name);
}
const boundInst: any = new BoundPoint(20);
console.log("bound-x=" + boundInst.x);
console.log("bound-y=" + boundInst.y);
console.log("bound-instanceof-point=" + (boundInst instanceof (Point as any)));

// A Proxy over a class is extensible: the trap-free proxy forwards everything.
const ProxiedBase: any = new Proxy(Base, {});
class ProxySub extends ProxiedBase {
  label: string = "ps";
}
const ps: any = new ProxySub();
console.log("proxy-tag=" + ps.tag);
console.log("proxy-label=" + ps.label);
console.log("proxy-instanceof-base=" + (ps instanceof Base));
console.log("proxy-keys=" + Object.keys(ps).join(","));

// A non-constructor heritage is refused at definition time.
try {
  const arrow = () => 1;
  class BadArrow extends (arrow as any) {}
  console.log("extends-arrow=no-throw");
} catch (e: any) {
  console.log("extends-arrow=" + e.constructor.name);
}
try {
  class BadNumber extends (7 as any) {}
  console.log("extends-number=no-throw");
} catch (e: any) {
  console.log("extends-number=" + e.constructor.name);
}
try {
  class BadUndef extends (undefined as any) {}
  console.log("extends-undefined=no-throw");
} catch (e: any) {
  console.log("extends-undefined=" + e.constructor.name);
}

// A heritage whose .prototype is a primitive gives a null-prototype instance.
function WeirdProto(this: any) {
  this.k = 1;
}
(WeirdProto as any).prototype = 5;
try {
  class WeirdSub extends (WeirdProto as any) {}
  console.log("weird-proto=no-throw");
} catch (e: any) {
  console.log("weird-proto=" + e.constructor.name);
}
