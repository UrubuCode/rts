// Cross-runtime: instance fields initialize in declaration order and are
// visible to later initializers and to the constructor body.
const order: string[] = [];

function mark(tag: string, value: number): number {
  order.push(tag);
  return value;
}

class Rec {
  a: number = mark("a", 1);
  b: number = mark("b", this.a + 1);
  c: number = mark("c", this.a + this.b);
  d: string = "a=" + this.a + ",b=" + this.b + ",c=" + this.c;
  e: number;

  constructor(seed: number) {
    order.push("ctor");
    this.e = this.c + seed;
  }
}

const r = new Rec(10);
console.log("order=" + order.join("|"));
console.log("a=" + r.a);
console.log("b=" + r.b);
console.log("c=" + r.c);
console.log("d=" + r.d);
console.log("e=" + r.e);
console.log("keys=" + Object.keys(r).join(","));

// A field with no initializer still exists (undefined) and keeps its slot
class Sparse {
  x: number = 1;
  y: number | undefined;
  z: number = 3;
}
const s = new Sparse();
console.log("sparse_keys=" + Object.keys(s).join(","));
console.log("sparse_y=" + String(s.y));
console.log("sparse_has_y=" + Object.prototype.hasOwnProperty.call(s, "y"));

// Field initializers run once per instance (fresh array each time)
class Owner {
  items: number[] = [];
  constructor(n: number) {
    this.items.push(n);
  }
}
const o1 = new Owner(1);
const o2 = new Owner(2);
console.log("o1=" + o1.items.join(",") + " o2=" + o2.items.join(","));
console.log("shared=" + (o1.items === o2.items));

// An own field shadows a prototype method of the same name
class Collide {
  hit: () => string = () => "field";
  hitProto(): string {
    return "proto";
  }
}
const cl = new Collide();
console.log("collide=" + cl.hit() + "/" + cl.hitProto());
