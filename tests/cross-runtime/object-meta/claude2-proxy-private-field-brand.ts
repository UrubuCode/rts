// Pins the one property lookup a proxy can never forward: a PRIVATE name is
// not a property key, so `#x in proxy` is false, a method reading `this.#x`
// with the proxy as receiver throws, and no handler can repair it.

class Counter {
  #count = 0;
  static #shared = "S";
  label: string;

  constructor(label: string) { this.label = label; }

  bump(): number { this.#count++; return this.#count; }
  read(): number { return this.#count; }
  static hasBrand(o: any): boolean { return #count in o; }
  static readShared(): string { return this.#shared; }
  get viaGetter(): number { return this.#count; }
}

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const c = new Counter("real");
c.bump();
c.bump();

const forwarding: any = {
  get(t: any, k: any, r: any) { return Reflect.get(t, k, r); },
  set(t: any, k: any, v: any, r: any) { return Reflect.set(t, k, v, r); },
  has(t: any, k: any) { return Reflect.has(t, k); },
};
const p: any = new Proxy(c, forwarding);
const bare: any = new Proxy(c, {});
const nested: any = new Proxy(bare, {});

console.log("target_read=" + c.read());
console.log("brand_target=" + Counter.hasBrand(c));
console.log("brand_proxy=" + Counter.hasBrand(p));
console.log("brand_bare=" + Counter.hasBrand(bare));
console.log("brand_nested=" + Counter.hasBrand(nested));
console.log("brand_plain=" + Counter.hasBrand({}));
console.log("public_forwards=" + p.label + ":" + ("label" in p));

// calling a method with the proxy as `this` fails on the private read
attempt("proxy_read", () => String(p.read()));
attempt("proxy_bump", () => String(p.bump()));
attempt("proxy_getter", () => String(p.viaGetter));
attempt("bare_read", () => String(bare.read()));
attempt("nested_read", () => String(nested.read()));

// the method itself is fine: only the receiver is wrong
console.log("via_target=" + Reflect.apply(p.read, c, []));
console.log("via_call=" + (p.read as any).call(c));
console.log("target_untouched=" + c.read());

// a handler that binds every method to the target repairs the receiver, which
// is the only fix — nothing at the trap level can see a private name
const repaired: any = new Proxy(c, {
  get(t: any, k: any, r: any) {
    const v = Reflect.get(t, k, r);
    return typeof v === "function" ? v.bind(t) : v;
  },
});
console.log("repaired_read=" + repaired.read());
console.log("repaired_bump=" + repaired.bump());
attempt("repaired_getter", () => String(repaired.viaGetter));

// a private name never shows up as a key, however the proxy is asked
console.log("ownKeys=" + Reflect.ownKeys(p).map(String).join("|"));
console.log("keys=" + Object.keys(p).join("|"));
console.log("json=" + JSON.stringify(p));
console.log("gopd_count=" + String(Object.getOwnPropertyDescriptor(p, "count")));
console.log("get_count=" + String(p.count));

// a static private name is on the class object, so a proxy of the CLASS fails
// the same way
const PCounter: any = new Proxy(Counter, forwarding);
console.log("static_via_class=" + Counter.readShared());
attempt("static_via_proxy", () => String(PCounter.readShared()));
console.log("static_brand_class=" + (() => { try { return String(PCounter.hasBrand(c)); } catch (e: any) { return "throw:" + e.constructor.name; } })());

// an instance built THROUGH a proxy of the class does carry the brand: the
// construct forwards to the real class
const built: any = new PCounter("built");
console.log("built_brand=" + Counter.hasBrand(built));
console.log("built_read=" + built.read());
console.log("built_is_proxy=" + (built instanceof Counter));

// a subclass instance carries the private name of the base it was built from
class Sub extends Counter { constructor() { super("sub"); } }
const s = new Sub();
console.log("sub_brand=" + Counter.hasBrand(s));
console.log("sub_proxy_brand=" + Counter.hasBrand(new Proxy(s, {})));
