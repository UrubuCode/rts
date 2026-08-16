// Cross-runtime: class bodies are strict, so assigning through a getter-only
// accessor throws; `super.prop` reads the home object's prototype with `this`
// as receiver, and a nested arrow keeps that home object.
class Base {
  get label(): string {
    return "base:" + (this as any).tag;
  }
  set label(v: string) {
    (this as any).tag = "set(" + v + ")";
  }
  get readOnly(): string {
    return "ro";
  }
  greet(): string {
    return "base-greet:" + (this as any).tag;
  }
  static origin(): string {
    return "base-static";
  }
}

class Derived extends Base {
  tag: string = "d";

  viaSuper(): string {
    return super.label;
  }
  viaSuperArrow(): string {
    const f = () => super.label + "|" + super.greet();
    return f();
  }
  writeSuper(v: string): string {
    super.label = v;
    return this.tag;
  }
  ownWrite(v: string): string {
    // Assigning `this.label` finds the inherited accessor pair and calls the setter.
    (this as any).label = v;
    return this.tag;
  }
  breakReadOnly(): string {
    try {
      (this as any).readOnly = "x";
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static origin(): string {
    return "derived+" + super.origin();
  }
}

const d = new Derived();
console.log("via-super=" + d.viaSuper());
console.log("via-super-arrow=" + d.viaSuperArrow());
console.log("write-super=" + d.writeSuper("A"));
console.log("own-write=" + d.ownWrite("B"));
console.log("read-only=" + d.breakReadOnly());
console.log("static-super=" + Derived.origin());
console.log("keys=" + Object.keys(d).join(","));

// The setter wrote onto the instance, so the accessor pair is still on Base.
console.log("own-label=" + (Object.getOwnPropertyDescriptor(d, "label") === undefined));
console.log("proto-label=" + (Object.getOwnPropertyDescriptor(Base.prototype, "label") !== undefined));

// A getter defined in the class body with no setter: sloppy-free, always throws.
class GetterOnly {
  get v(): number {
    return 1;
  }
}
const g = new GetterOnly();
// The write happens inside a class body, which is strict whatever the module
// goal of the enclosing file is.
class Writer {
  static write(target: any, key: string, value: any): string {
    try {
      target[key] = value;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
}
console.log("assign-getter-only=" + Writer.write(g, "v", 2));
console.log("getter-only-value=" + g.v);

// A pair split across two objects: the get on the prototype, the set added later.
const proto: any = {};
Object.defineProperty(proto, "half", {
  get(): string {
    return "get-half";
  },
  configurable: true,
});
const child: any = Object.create(proto);
console.log("split-strict=" + Writer.write(child, "half", 1));
Object.defineProperty(proto, "half", {
  get(): string {
    return "get-half";
  },
  set(this: any, v: any): void {
    this.stored = v;
  },
  configurable: true,
});
child.half = 9;
console.log("split-after=" + child.half + ":" + child.stored);

// `super` in an object-literal method uses the literal's [[HomeObject]], which
// Object.setPrototypeOf can retarget afterwards.
const litProto = {
  who(): string {
    return "proto-who";
  },
};
const lit = {
  who(): string {
    return "lit+" + (super.who as any)();
  },
};
Object.setPrototypeOf(lit, litProto);
console.log("literal-super=" + lit.who());

const litProto2 = {
  who(): string {
    return "proto2-who";
  },
};
Object.setPrototypeOf(lit, litProto2);
console.log("literal-super-retargeted=" + lit.who());

// super is resolved against the home object, not against the receiver's chain.
const borrowed: any = { tag: "borrowed" };
borrowed.viaSuper = Derived.prototype.viaSuper;
console.log("borrowed=" + borrowed.viaSuper());
console.log("borrowed-proto=" + (Object.getPrototypeOf(borrowed) === Object.prototype));

// A getter and its setter can live on different objects of one chain; the
// lookup finds whichever comes first and never merges the two.
const lower: any = {};
Object.defineProperty(lower, "split", { get: () => "lower-get", configurable: true });
const upper: any = Object.create(lower);
Object.defineProperty(upper, "split", { set(this: any, v: any) { this.saw = v; }, configurable: true });
console.log("split-read=" + String(upper.split));
console.log("split-write=" + Writer.write(upper, "split", 3) + ":" + upper.saw);

// A class accessor pair declared out of order is still one property.
class OutOfOrder {
  set p(v: number) {
    (this as any).store = v * 2;
  }
  get p(): number {
    return (this as any).store;
  }
}
const oo: any = new OutOfOrder();
oo.p = 4;
console.log("out-of-order=" + oo.p);
const ood: any = Object.getOwnPropertyDescriptor(OutOfOrder.prototype, "p");
console.log("out-of-order-pair=" + (typeof ood.get) + ":" + (typeof ood.set));

// A getter redeclared later in the body replaces only the get half; the setter
// declared before it survives on the same accessor property.
class Redeclared {
  get q(): string {
    return "first";
  }
  set q(v: string) {
    (this as any).seen = v;
  }
  get q(): string {
    return "second";
  }
}
const rd: any = new Redeclared();
console.log("redeclared-get=" + rd.q);
const rdd: any = Object.getOwnPropertyDescriptor(Redeclared.prototype, "q");
console.log("redeclared-set=" + (rdd.set === undefined ? "dropped" : "kept"));
console.log("redeclared-write=" + Writer.write(rd, "q", "x"));

// super in a static method reads the base CONSTRUCTOR, not the base prototype.
console.log("static-lookup=" + (Object.getPrototypeOf(Derived) === Base));
console.log("proto-lookup=" + (Object.getPrototypeOf(Derived.prototype) === Base.prototype));
