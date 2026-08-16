// Cross-runtime: the TWO chains a class hierarchy has. Instances walk
// Derived.prototype -> Base.prototype -> Object.prototype -> null, and the
// constructors themselves walk Derived -> Base -> Function.prototype -> null.
// `extends null` and a plain class differ only in where the first chain stops.
class Root {
  r(): string {
    return "r";
  }
  static sr(): string {
    return "sr";
  }
}
class Mid extends Root {
  m(): string {
    return "m";
  }
  static sm(): string {
    return "sm";
  }
}
class Leaf extends Mid {
  l(): string {
    return "l";
  }
}

const leaf = new Leaf();

// Instance chain, walked one link at a time.
const chain: string[] = [];
let cur: any = leaf;
for (let i = 0; i < 8; i++) {
  cur = Object.getPrototypeOf(cur);
  if (cur === null) {
    chain.push("null");
    break;
  }
  chain.push(cur === Leaf.prototype ? "Leaf.prototype"
    : cur === Mid.prototype ? "Mid.prototype"
    : cur === Root.prototype ? "Root.prototype"
    : cur === Object.prototype ? "Object.prototype"
    : "other");
}
console.log("instance-chain=" + chain.join(">"));

// Constructor (static) chain.
const statics: string[] = [];
let sc: any = Leaf;
for (let i = 0; i < 8; i++) {
  sc = Object.getPrototypeOf(sc);
  if (sc === null) {
    statics.push("null");
    break;
  }
  statics.push(sc === Mid ? "Mid" : sc === Root ? "Root"
    : sc === Function.prototype ? "Function.prototype"
    : sc === Object.prototype ? "Object.prototype" : "other");
}
console.log("static-chain=" + statics.join(">"));

console.log("leaf-static-sr=" + Leaf.sr());
console.log("leaf-static-sm=" + Leaf.sm());
console.log("leaf-methods=" + leaf.l() + leaf.m() + leaf.r());
console.log("isprototypeof=" + Root.prototype.isPrototypeOf(leaf) + "," + Mid.prototype.isPrototypeOf(leaf));
console.log("root-not-of-mid-inst=" + Leaf.prototype.isPrototypeOf(new Mid()));

// A base class with no extends sits directly under Function.prototype.
console.log("root-static-proto=" + (Object.getPrototypeOf(Root) === Function.prototype));
console.log("root-proto-proto=" + (Object.getPrototypeOf(Root.prototype) === Object.prototype));

// `prototype` on a constructor is non-writable, non-enumerable,
// non-configurable; `constructor` on the prototype is the writable one.
const pd: any = Object.getOwnPropertyDescriptor(Leaf, "prototype");
console.log("prototype-desc=w" + pd.writable + ",e" + pd.enumerable + ",c" + pd.configurable);
const cd: any = Object.getOwnPropertyDescriptor(Leaf.prototype, "constructor");
console.log("constructor-desc=w" + cd.writable + ",e" + cd.enumerable + ",c" + cd.configurable);
console.log("own-names-on-class=" + Object.getOwnPropertyNames(Leaf).sort().join(","));

// extends null: the instance chain stops at the class prototype itself.
class Bare extends null {
  constructor() {
    // A derived constructor with a null parent may never call super(); it must
    // produce `this` by returning an object instead.
    return Object.create(Bare.prototype);
  }
}
const bare: any = new Bare();
console.log("bare-proto-is-class=" + (Object.getPrototypeOf(bare) === Bare.prototype));
console.log("bare-proto-of-proto=" + (Object.getPrototypeOf(Bare.prototype) === null));
console.log("bare-static-proto=" + (Object.getPrototypeOf(Bare) === Function.prototype));
console.log("bare-has-tostring=" + (typeof bare.toString));
console.log("bare-instanceof=" + (bare instanceof Bare));

// Object.create rewires the instance chain without touching the class.
const rewired: any = Object.create(Root.prototype);
console.log("rewired-r=" + rewired.r());
console.log("rewired-instanceof=" + (rewired instanceof Root) + "," + (rewired instanceof Mid));
console.log("rewired-ctor=" + (rewired.constructor === Root));

// setPrototypeOf on the STATIC side alone moves static lookup, not instances.
class Donor {
  static gift(): string {
    return "gift";
  }
}
class Taker {}
Object.setPrototypeOf(Taker, Donor);
console.log("taker-gift=" + (Taker as any).gift());
console.log("taker-instance-chain=" + (Object.getPrototypeOf(Taker.prototype) === Object.prototype));
console.log("taker-instance-gift=" + (typeof (new Taker() as any).gift));
