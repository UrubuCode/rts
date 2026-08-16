// Pins how a Proxy LOOKS UP a trap: an absent trap, one set to undefined and
// one set to null all forward to the target (GetMethod treats null as absent),
// a non-callable trap is a TypeError, and the handler property is read afresh
// through [[Get]] on every operation — so an accessor handler works.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const base: any = { a: 1 };

// the three shapes that all mean "no trap"
console.log("absent=" + (new Proxy(base, {}) as any).a);
console.log("undefined=" + (new Proxy(base, { get: undefined }) as any).a);
console.log("null=" + (new Proxy(base, { get: null as any }) as any).a);

// anything else non-callable is refused at the point of use, not at construction
console.log("built_number=" + typeof new Proxy(base, { get: 1 as any }));
attempt("use_number", () => String((new Proxy(base, { get: 1 as any }) as any).a));
attempt("use_string", () => String((new Proxy(base, { get: "x" as any }) as any).a));
attempt("use_object", () => String((new Proxy(base, { get: {} as any }) as any).a));
attempt("use_array", () => String((new Proxy(base, { get: [] as any }) as any).a));
attempt("use_symbol", () => String((new Proxy(base, { get: Symbol("s") as any }) as any).a));

// a callable that is itself a proxy is a perfectly good trap
const callableTrap: any = new Proxy(function () { return "VIA_PROXY_TRAP"; }, {});
console.log("proxy_trap=" + (new Proxy(base, { get: callableTrap }) as any).a);
// so is a bound function and a class method
const boundTrap: any = (function (this: any) { return "BOUND:" + this.tag; }).bind({ tag: "B" });
console.log("bound_trap=" + (new Proxy(base, { get: boundTrap }) as any).a);
// but a class constructor is callable only with new, so it throws when applied
class Ctor { }
attempt("class_trap", () => String((new Proxy(base, { get: Ctor as any }) as any).a));

// null and undefined forward for every trap, not just get
const nulled: any = new Proxy({ k: 1 }, {
  get: null as any, set: null as any, has: null as any, deleteProperty: null as any,
  ownKeys: null as any, getOwnPropertyDescriptor: null as any, defineProperty: null as any,
  getPrototypeOf: null as any, setPrototypeOf: null as any,
  isExtensible: null as any, preventExtensions: null as any,
});
console.log("null_get=" + nulled.k);
console.log("null_has=" + ("k" in nulled));
console.log("null_set=" + Reflect.set(nulled, "k", 2) + ",v=" + nulled.k);
console.log("null_keys=" + Reflect.ownKeys(nulled).join("|"));
console.log("null_gopd=" + (Object.getOwnPropertyDescriptor(nulled, "k") as any).value);
console.log("null_define=" + Reflect.defineProperty(nulled, "n", { value: 3, configurable: true }));
console.log("null_delete=" + Reflect.deleteProperty(nulled, "n"));
console.log("null_proto=" + (Reflect.getPrototypeOf(nulled) === Object.prototype));
console.log("null_ext=" + Reflect.isExtensible(nulled));

// a non-callable trap is refused per OPERATION: the same handler is fine for
// the traps that are callable
const mixed: any = new Proxy({ m: 1 }, { get: 5 as any, has() { return true; } });
console.log("mixed_has=" + ("anything" in mixed));
attempt("mixed_get", () => String(mixed.m));

// the handler is consulted through [[Get]] every time, so a getter counts calls
let reads = 0;
const dynamic: any = {};
Object.defineProperty(dynamic, "get", {
  get() { reads++; return () => "call" + reads; },
  configurable: true,
});
const dyn: any = new Proxy(base, dynamic);
console.log("dyn1=" + dyn.a);
console.log("dyn2=" + dyn.a);
console.log("dyn3=" + dyn.b);
console.log("dyn_reads=" + reads);

// and a handler mutated between operations changes behaviour immediately
const live: any = { get() { return "first"; } };
const lp: any = new Proxy(base, live);
console.log("live1=" + lp.a);
live.get = function () { return "second"; };
console.log("live2=" + lp.a);
delete live.get;
console.log("live3=" + lp.a);
live.get = 7;
attempt("live4", () => String(lp.a));
