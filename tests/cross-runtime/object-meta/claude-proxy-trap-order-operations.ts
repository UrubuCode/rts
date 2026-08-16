// Pins the exact trap SEQUENCE each meta-operation performs on a proxy:
// ownKeys once, then getOwnPropertyDescriptor per key to filter by enumerable,
// then get per surviving key — and which operations skip a step. 98 logs traps
// but only for a single Object.keys call.

const log: string[] = [];

function makeProxy(): any {
  const target: any = { a: 1, b: 2 };
  Object.defineProperty(target, "hidden", { value: 3, enumerable: false, configurable: true, writable: true });
  return new Proxy(target, {
    ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
    get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
    has(t, k) { log.push("has:" + String(k)); return Reflect.has(t, k); },
    set(t, k, v, r) { log.push("set:" + String(k)); return Reflect.set(t, k, v, r); },
    defineProperty(t, k, d) { log.push("define:" + String(k)); return Reflect.defineProperty(t, k, d); },
    deleteProperty(t, k) { log.push("delete:" + String(k)); return Reflect.deleteProperty(t, k); },
    getPrototypeOf(t) { log.push("getProto"); return Reflect.getPrototypeOf(t); },
  });
}

function run(label: string, fn: (p: any) => void): void {
  log.length = 0;
  const p = makeProxy();
  fn(p);
  console.log(label + "=" + log.join(","));
}

run("ownKeys", (p) => { Reflect.ownKeys(p); });
run("getOwnPropertyNames", (p) => { Object.getOwnPropertyNames(p); });
run("keys", (p) => { Object.keys(p); });
run("values", (p) => { Object.values(p); });
run("entries", (p) => { Object.entries(p); });
run("assign_from", (p) => { Object.assign({}, p); });
run("spread", (p) => { const _x = { ...p }; void _x; });
run("json", (p) => { JSON.stringify(p); });
run("forin", (p) => { for (const _k in p) { void _k; } });
run("getOwnPropertyDescriptors", (p) => { Object.getOwnPropertyDescriptors(p); });
run("freeze", (p) => { Object.freeze(p); });
run("seal", (p) => { Object.seal(p); });
run("isFrozen", (p) => { Object.isFrozen(p); });
run("hasOwn", (p) => { Object.hasOwn(p, "a"); });
run("in", (p) => { void ("a" in p); });
run("dot_read", (p) => { void p.a; });
run("dot_write", (p) => { p.c = 3; });
run("define", (p) => { Object.defineProperty(p, "d", { value: 4, configurable: true }); });
run("delete", (p) => { delete p.a; });
run("instanceof", (p) => { void (p instanceof Object); });

// spread of a proxy whose ownKeys invents a key with no descriptor: no get
log.length = 0;
const invented = new Proxy({} as any, {
  ownKeys() { log.push("ownKeys"); return ["ghost", "real"]; },
  getOwnPropertyDescriptor(_t, k) {
    log.push("gopd:" + String(k));
    return String(k) === "real" ? { value: 1, enumerable: true, configurable: true, writable: true } : undefined;
  },
  get(_t, k) { log.push("get:" + String(k)); return "v"; },
});
const spreadResult = { ...invented };
console.log("invented_trace=" + log.join(","));
console.log("invented_keys=" + Object.keys(spreadResult).join("|"));

// a non-enumerable reported key stops before the get
log.length = 0;
const nonEnum = new Proxy({} as any, {
  ownKeys() { log.push("ownKeys"); return ["ne", "e"]; },
  getOwnPropertyDescriptor(_t, k) {
    log.push("gopd:" + String(k));
    return { value: 1, enumerable: String(k) === "e", configurable: true, writable: true };
  },
  get(_t, k) { log.push("get:" + String(k)); return "v"; },
});
console.log("nonenum_keys=" + Object.keys(nonEnum).join("|"));
console.log("nonenum_trace=" + log.join(","));

// symbols are reported by ownKeys but skipped by the string-only operations
log.length = 0;
const sym = Symbol("s");
const withSym = new Proxy({ a: 1, [sym]: 2 } as any, {
  ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t, k) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
});
Object.keys(withSym);
console.log("sym_keys_trace=" + log.join(","));
log.length = 0;
const _s = { ...withSym };
void _s;
console.log("sym_spread_trace=" + log.join(","));
