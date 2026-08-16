// Cross-runtime: the mixin pattern — a function returning a class expression
// that extends its argument. Each application builds a real link in the
// prototype chain, super() threads through every one of them, and applying the
// same mixin twice yields two independent classes.
const order: string[] = [];

function Serialisable(BaseCtor: any): any {
  return class Serialisable extends BaseCtor {
    serialisedBy: string = "serialisable";
    constructor(...args: any[]) {
      order.push("Serialisable.before");
      super(...args);
      order.push("Serialisable.after");
    }
    describe(): string {
      return "S<" + (super.describe ? super.describe() : "-") + ">";
    }
    static kind(): string {
      return "serialisable";
    }
  };
}

function Countable(BaseCtor: any): any {
  return class Countable extends BaseCtor {
    counted: number = 0;
    constructor(...args: any[]) {
      order.push("Countable.before");
      super(...args);
      this.counted = args.length;
      order.push("Countable.after");
    }
    describe(): string {
      return "C<" + (super.describe ? super.describe() : "-") + ">";
    }
  };
}

class Core {
  name: string = "";
  constructor(name: string) {
    order.push("Core.ctor");
    this.name = name;
  }
  describe(): string {
    return "Core(" + this.name + ")";
  }
}

const Composed: any = Countable(Serialisable(Core));
const c = new Composed("thing", "extra");

console.log("describe=" + c.describe());
console.log("name=" + c.name);
console.log("counted=" + c.counted);
console.log("serialised-by=" + c.serialisedBy);
console.log("keys=" + Object.keys(c).join(","));
console.log("order=" + order.join(">"));

console.log("instanceof-core=" + (c instanceof Core));
console.log("instanceof-composed=" + (c instanceof Composed));
console.log("chain-len=" + chainLength(c));
console.log("static-kind=" + Composed.kind());
console.log("static-inherited=" + Object.prototype.hasOwnProperty.call(Composed, "kind"));

function chainLength(o: any): number {
  let n = 0;
  let cur = Object.getPrototypeOf(o);
  while (cur !== null && n < 20) {
    n++;
    cur = Object.getPrototypeOf(cur);
  }
  return n;
}

// The class expression's own name is what the mixin body wrote, and it is
// visible from inside the class.
console.log("composed-name=" + Composed.name);
console.log("inner-name=" + Object.getPrototypeOf(Composed).name);
console.log("ctor-of-proto=" + (Composed.prototype.constructor === Composed));

// Applying a mixin twice makes two unrelated classes over the same base.
const A1: any = Serialisable(Core);
const A2: any = Serialisable(Core);
console.log("two-applications-distinct=" + (A1 !== A2));
console.log("prototypes-distinct=" + (A1.prototype !== A2.prototype));
console.log("both-extend-core=" + (Object.getPrototypeOf(A1.prototype) === Core.prototype) + "," + (Object.getPrototypeOf(A2.prototype) === Core.prototype));
const a1 = new A1("one");
console.log("a1-instanceof-a1=" + (a1 instanceof A1));
console.log("a1-instanceof-a2=" + (a1 instanceof A2));
console.log("a1-describe=" + a1.describe());

// A subclass declared over a composed class keeps the whole chain.
class Leaf extends Composed {
  leafTag: string = "leaf";
  describe(): string {
    return "L<" + super.describe() + ">";
  }
}
order.length = 0;
const leaf = new Leaf("deep");
console.log("leaf-describe=" + leaf.describe());
console.log("leaf-order=" + order.join(">"));
console.log("leaf-keys=" + Object.keys(leaf).join(","));
console.log("leaf-chain-len=" + chainLength(leaf));
console.log("leaf-instanceof=" + (leaf instanceof Core) + "," + (leaf instanceof Composed) + "," + (leaf instanceof Leaf));
console.log("leaf-static-kind=" + (Leaf as any).kind());

// Each mixin layer contributes exactly one `describe` to the chain.
const owners: string[] = [];
let walk: any = Object.getPrototypeOf(leaf);
while (walk !== null && owners.length < 10) {
  if (Object.prototype.hasOwnProperty.call(walk, "describe")) {
    owners.push(walk.constructor.name);
  }
  walk = Object.getPrototypeOf(walk);
}
console.log("describe-owners=" + owners.join(","));
