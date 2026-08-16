// Cross-runtime: what a field initialiser can SEE. Prototype methods already
// exist when the first field runs, a later field is still undefined, the class
// binding and outer bindings are in scope, and every instance gets its own
// evaluation of the initialiser expression.
const outer = "outer-value";
const built: string[] = [];

class Node2 {
  // Calls a prototype method: methods are installed before any field runs.
  viaMethod: string = this.stamp("first");
  // Reads a field declared BEFORE it, and one declared after (undefined).
  fromEarlier: string = "sees:" + this.viaMethod;
  fromLater: string = "later-is:" + String((this as any).declaredLast);
  // The class binding itself is in scope.
  fromClass: string = "class:" + Node2.label;
  // So is the enclosing lexical scope.
  fromOuter: string = outer;
  // A private method is reachable too.
  fromPrivate: string = this.#secret();
  // Fresh object per instance, not shared.
  bag: string[] = [];
  declaredLast: string = "last";

  static label: string = "N";

  stamp(tag: string): string {
    built.push(tag);
    return "stamp-" + tag;
  }

  #secret(): string {
    return "private-ok";
  }
}

const a = new Node2();
const b = new Node2();

console.log("via-method=" + a.viaMethod);
console.log("from-earlier=" + a.fromEarlier);
console.log("from-later=" + a.fromLater);
console.log("from-class=" + a.fromClass);
console.log("from-outer=" + a.fromOuter);
console.log("from-private=" + a.fromPrivate);
console.log("declared-last=" + a.declaredLast);
console.log("built=" + built.join(","));

a.bag.push("a");
b.bag.push("b1");
b.bag.push("b2");
console.log("bag-shared=" + (a.bag === b.bag));
console.log("bag-a=" + a.bag.join(","));
console.log("bag-b=" + b.bag.join(","));

// Declaration order is the own-key order on the instance.
console.log("keys=" + Object.keys(a).join(","));
console.log("keys-len=" + Object.keys(a).length);

// A field is an own enumerable, writable, configurable data property; the
// method it called is a non-enumerable prototype property.
const fd: any = Object.getOwnPropertyDescriptor(a, "viaMethod");
console.log("field-desc=w" + fd.writable + ",e" + fd.enumerable + ",c" + fd.configurable);
const md: any = Object.getOwnPropertyDescriptor(Node2.prototype, "stamp");
console.log("method-desc=w" + md.writable + ",e" + md.enumerable + ",c" + md.configurable);
console.log("method-own-on-instance=" + Object.prototype.hasOwnProperty.call(a, "stamp"));

// A field initialiser in a DERIVED class runs after super() and can see what
// the base constructor already wrote.
class Base2 {
  seed: number = 1;
  constructor() {
    this.seed = 5;
    (this as any).fromCtor = "ctor-wrote";
  }
}
class Child2 extends Base2 {
  observed: string = "seed:" + this.seed + ",ctor:" + String((this as any).fromCtor);
  doubled: number = this.seed * 2;
}
const c = new Child2();
console.log("child-observed=" + c.observed);
console.log("child-doubled=" + c.doubled);
console.log("child-keys=" + Object.keys(c).join(","));

// Arrow initialisers capture the instance, and are per-instance functions.
class Counter2 {
  n: number = 0;
  bump: () => number = () => ++this.n;
}
const c1 = new Counter2();
const c2 = new Counter2();
const detached = c1.bump;
console.log("arrow-detached=" + detached() + "," + detached());
console.log("arrow-other=" + c2.bump());
console.log("arrow-distinct=" + (c1.bump !== c2.bump));
console.log("arrow-own=" + Object.prototype.hasOwnProperty.call(c1, "bump"));
console.log("arrow-name=" + c1.bump.name);
console.log("counter-values=" + c1.n + "," + c2.n);
