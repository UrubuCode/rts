// Pins a class whose SUPERCLASS is a proxy: defining the class reads
// "prototype" through the get trap once, `super(...)` reaches the construct
// trap with newTarget set to the DERIVED class, and static members resolve
// through the get trap on every access.

const log: string[] = [];

class Base {
  tag: string;
  constructor(v: string) { this.tag = "base:" + v; }
  static make(): string { return "static_make"; }
  static kind = "BASE";
  hello(): string { return "hello:" + this.tag; }
}

const PBase: any = new Proxy(Base, {
  get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
  construct(t, args, nt) {
    log.push("construct:" + args.join("/") + ":nt=" + (nt === PBase ? "proxy" : nt === (Derived as any) ? "Derived" : "?"));
    return Reflect.construct(t as any, args, nt);
  },
});

log.length = 0;
class Derived extends PBase {
  extra: string;
  constructor() {
    super("d");
    this.extra = "E";
  }
  hello(): string { return "derived:" + super.hello(); }
}
console.log("class_definition=" + log.join(","));

log.length = 0;
const d: any = new Derived();
console.log("new_derived_log=" + log.join(","));
console.log("instance=" + d.tag + "|" + d.extra);
console.log("instanceof_derived=" + (d instanceof Derived));
console.log("instanceof_base=" + (d instanceof Base));
console.log("instanceof_proxy=" + (d instanceof PBase));
console.log("proto_chain=" + (Object.getPrototypeOf(Object.getPrototypeOf(d)) === Base.prototype));
console.log("super_method=" + d.hello());

// the class object itself inherits from the PROXY, so statics go through get
log.length = 0;
console.log("static_kind=" + Derived.kind);
console.log("static_call=" + (Derived as any).make());
console.log("static_log=" + log.join(","));
console.log("class_proto_is_proxy=" + (Object.getPrototypeOf(Derived) === PBase));

// constructing the proxy directly reports itself as newTarget
log.length = 0;
const direct: any = new PBase("x");
console.log("direct_log=" + log.join(","));
console.log("direct_tag=" + direct.tag + ",instanceof=" + (direct instanceof Base));

// a construct trap that returns an unrelated object wins over super()'s binding
class Hijacked extends (new Proxy(Base, { construct() { return { tag: "HIJACKED", extra: "H" }; } }) as any) {
  constructor() { super("ignored"); }
}
const h: any = new Hijacked();
console.log("hijacked=" + h.tag + "|" + h.extra);
console.log("hijacked_instanceof=" + (h instanceof Hijacked) + ":" + (h instanceof Base));

// a construct trap returning a primitive breaks the derived constructor too
class BadSuper extends (new Proxy(Base, { construct() { return 5 as any; } }) as any) {
  constructor() { super("x"); }
}
try {
  const _b = new BadSuper();
  console.log("bad_super=ok");
} catch (e: any) {
  console.log("bad_super=throw:" + e.constructor.name);
}

// a get trap handing back a different "prototype" is only legal when the
// target's own slot allows it: a class's prototype is non-writable AND
// non-configurable, so the get invariant refuses the substitution outright,
// while a plain function's is writable and the lie goes through
function makeWeird(base: any, fake: any): string {
  try {
    const P: any = new Proxy(base, { get(t, k, r) { return k === "prototype" ? fake : Reflect.get(t, k, r); } });
    const C: any = class extends P { constructor() { super("w"); } };
    const inst: any = new C();
    return "chain=" + (Object.getPrototypeOf(C.prototype) === fake) + ",fake=" + inst.fake +
      ",tag=" + inst.tag + ",instanceof_base=" + (inst instanceof base);
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
}
const fakeProto: any = { fake: "FAKE" };
function PlainBase(this: any, v: string) { (this as any).tag = "plain:" + v; }
console.log("weird_class=" + makeWeird(Base, fakeProto));
console.log("weird_function=" + makeWeird(PlainBase, fakeProto));
const classDesc = Object.getOwnPropertyDescriptor(Base, "prototype") as any;
console.log("class_proto_desc=w=" + classDesc.writable + ",e=" + classDesc.enumerable + ",c=" + classDesc.configurable);
const plainDesc = Object.getOwnPropertyDescriptor(PlainBase, "prototype") as any;
console.log("fn_proto_desc=w=" + plainDesc.writable + ",e=" + plainDesc.enumerable + ",c=" + plainDesc.configurable);

// a proxy of a class stays non-callable without new
try {
  const _r = (PBase as any)("nope");
  console.log("call_class=ok");
} catch (e: any) {
  console.log("call_class=throw:" + e.constructor.name);
}
