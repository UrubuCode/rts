// Pins WHICH trap names a proxy asks its handler for, and in what order, for
// each high-level operation — observed by making the handler itself a Proxy —
// and that a trap inherited from the handler's PROTOTYPE is found normally.

const looked: string[] = [];

function sniff(target: any): any {
  return new Proxy(target, new Proxy({} as any, {
    get(_h, k) { looked.push(String(k)); return undefined; },
  }));
}

function run(label: string, fn: (p: any) => void): void {
  looked.length = 0;
  const t: any = { a: 1, b: 2 };
  try {
    fn(sniff(t));
    console.log(label + "=" + looked.join(","));
  } catch (e: any) {
    console.log(label + "=" + looked.join(",") + "|throw:" + e.constructor.name);
  }
}

run("get", (p) => { void p.a; });
run("set", (p) => { Reflect.set(p, "a", 9); });
run("has", (p) => { void ("a" in p); });
run("delete", (p) => { Reflect.deleteProperty(p, "a"); });
run("ownKeys", (p) => { Reflect.ownKeys(p); });
run("keys", (p) => { Object.keys(p); });
run("values", (p) => { Object.values(p); });
run("descriptors", (p) => { Object.getOwnPropertyDescriptors(p); });
run("gopd", (p) => { Object.getOwnPropertyDescriptor(p, "a"); });
run("define", (p) => { Reflect.defineProperty(p, "c", { value: 3, configurable: true }); });
run("getproto", (p) => { Object.getPrototypeOf(p); });
run("setproto", (p) => { Reflect.setPrototypeOf(p, null); });
run("isExtensible", (p) => { Object.isExtensible(p); });
run("preventExtensions", (p) => { Reflect.preventExtensions(p); });
run("forin", (p) => { for (const _k in p) { void _k; } });
run("spread", (p) => { const _o = { ...p }; void _o; });
run("json", (p) => { JSON.stringify(p); });
run("assign_from", (p) => { Object.assign({}, p); });
run("assign_to", (p) => { Object.assign(p, { z: 1 }); });
run("seal", (p) => { Object.seal(p); });
run("isSealed", (p) => { Object.isSealed(p); });
run("hasOwn", (p) => { Object.hasOwn(p, "a"); });
run("string_coerce", (p) => { void String(p); });

// a callable target adds the two function traps
looked.length = 0;
const fnProxy: any = sniff(function f(x: number) { return x; });
fnProxy(1);
console.log("apply=" + looked.join(","));
looked.length = 0;
const ctorProxy: any = sniff(function C(this: any) { (this as any).v = 1; });
new ctorProxy();
console.log("construct=" + looked.join(","));
looked.length = 0;
const instProxy: any = sniff(function C2(this: any) { /* noop */ });
void ({} instanceof instProxy);
console.log("instanceof=" + looked.join(","));

// traps found on the handler's PROTOTYPE work exactly like own ones
const protoHandler: any = {
  get(_t: any, k: any) { return "proto:" + String(k); },
  has() { return true; },
  ownKeys() { return ["fromProto"]; },
};
const inherited: any = new Proxy({ a: 1 }, Object.create(protoHandler));
console.log("inherited_get=" + inherited.a);
console.log("inherited_has=" + ("zz" in inherited));
console.log("inherited_keys=" + Reflect.ownKeys(inherited).join("|"));

// an own trap shadows the inherited one, and deleting it uncovers it again
const shadowing: any = Object.create(protoHandler);
shadowing.get = function () { return "own"; };
const shadowed: any = new Proxy({ a: 1 }, shadowing);
console.log("shadow_own=" + shadowed.a);
delete shadowing.get;
console.log("shadow_uncovered=" + shadowed.a);

// a class instance is a legal handler: its methods live on the prototype
class Handler {
  get(_t: any, k: any): string { return "class:" + String(k); }
}
console.log("class_handler=" + (new Proxy({ a: 1 }, new Handler() as any) as any).a);

// a null-prototype handler with no traps forwards everything
const bareHandler: any = Object.create(null);
console.log("bare_handler=" + (new Proxy({ a: 42 }, bareHandler) as any).a);
