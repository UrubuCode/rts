// Cross-runtime: class field initialization order and interaction with constructor.

const log: string[] = [];

class Base {
  x = (log.push("Base.x"), 1);
  y = (log.push("Base.y"), 2);
  constructor() { log.push("Base.constructor"); }
}

class Child extends Base {
  z = (log.push("Child.z"), 3);
  constructor() {
    super();  // Base fields init before super() returns? No — fields init after super()
    log.push("Child.constructor");
    this.z = (log.push("Child.z.reassign"), 99);
  }
}

new Child();
// Order: Base.x, Base.y, Base.constructor, Child.z, Child.constructor, Child.z.reassign
console.log(log.join(","));

// Field initializer vs constructor assignment
const order: string[] = [];
class A {
  a = (order.push("a-field"), 10);
  b: number;
  c = (order.push("c-field"), 30);
  constructor() {
    order.push("constructor-start");
    this.b = (order.push("b-constructor"), 20);
    order.push("constructor-end");
  }
}
new A();
console.log(order.join(","));
// a-field,c-field,constructor-start,b-constructor,constructor-end

// Static field order
const slog: string[] = [];
class S {
  static a = (slog.push("S.a"), 1);
  static b = (slog.push("S.b"), 2);
  static {  slog.push("S.block"); }
  static c = (slog.push("S.c"), 3);
}
console.log(slog.join(","));  // S.a,S.b,S.block,S.c

// Inheritance: parent static fields before child static fields
const ilog: string[] = [];
class P { static pv = (ilog.push("P.pv"), "p"); }
class C extends P { static cv = (ilog.push("C.cv"), "c"); }
console.log(ilog.join(","));  // P.pv,C.cv

// Field uses previous field value
class Counter {
  start = 10;
  end = this.start * 2;  // can reference earlier field
  range = this.end - this.start;
}
const ct = new Counter();
console.log(ct.start, ct.end, ct.range);  // 10 20 10
