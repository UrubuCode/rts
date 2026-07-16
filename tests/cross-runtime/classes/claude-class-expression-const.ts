// Cross-runtime: class expression assigned to a const and used with new.
const Point = class {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
  sum(): number {
    return this.x + this.y;
  }
  toString(): string {
    return "(" + this.x + "," + this.y + ")";
  }
};

const p = new Point(2, 3);
console.log("sum=" + p.sum());
console.log("str=" + String(p));
console.log("instance=" + (p instanceof Point));
console.log("name=" + Point.name);
console.log("length=" + Point.length);

// named class expression: the name is visible inside, not outside
const Node2 = class Inner {
  depth: number;
  constructor(depth: number) {
    this.depth = depth;
  }
  self(): string {
    return Inner.name + ":" + this.depth;
  }
  static make(d: number): Inner {
    return new Inner(d);
  }
};
const n = new Node2(1);
console.log("named_self=" + n.self());
console.log("named_outer=" + Node2.name);
console.log("named_static=" + Node2.make(4).self());
console.log("named_instance=" + (Node2.make(4) instanceof Node2));

// class expression extending another class expression
const Base3 = class {
  tag(): string {
    return "base";
  }
};
const Derived3 = class extends Base3 {
  tag(): string {
    return "derived>" + super.tag();
  }
};
const d = new Derived3();
console.log("expr_extends=" + d.tag());
console.log("expr_chain=" + (d instanceof Base3) + "/" + (d instanceof Derived3));

// class expression returned from a factory: each call is a distinct class
function makeClass(prefix: string) {
  return class {
    id: number;
    constructor(id: number) {
      this.id = id;
    }
    label(): string {
      return prefix + "-" + this.id;
    }
  };
}
const Alpha = makeClass("a");
const Beta = makeClass("b");
console.log("factory_a=" + new Alpha(1).label());
console.log("factory_b=" + new Beta(2).label());
console.log("factory_distinct=" + (Alpha === Beta));
console.log("factory_cross=" + (new Alpha(1) instanceof Beta));

// class expression used inline / stored in an array
const classes = [makeClass("x"), makeClass("y")];
const built: string[] = [];
for (let i = 0; i < classes.length; i++) {
  built.push(new classes[i](i).label());
}
console.log("from_array=" + built.join(","));

// immediately constructed class expression
console.log("iife=" + new (class {
  v(): string {
    return "inline";
  }
})().v());
