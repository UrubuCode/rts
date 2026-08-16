// Pins Object.assign against proxies: as a SOURCE it is read by ownKeys +
// descriptor + get (skipping what the descriptor calls non-enumerable), as a
// TARGET it is written by the set trap alone, and a refusing set trap makes
// assign throw part-way with the earlier keys already written.

const log: string[] = [];

function tracedSource(target: any): any {
  return new Proxy(target, {
    ownKeys(t) { log.push("src:ownKeys"); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { log.push("src:gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
    get(t, k, r) { log.push("src:get:" + String(k)); return Reflect.get(t, k, r); },
  });
}

function tracedTarget(target: any): any {
  return new Proxy(target, {
    set(t, k, v, r) { log.push("dst:set:" + String(k) + "=" + String(v)); return Reflect.set(t, k, v, r); },
    getOwnPropertyDescriptor(t, k) { log.push("dst:gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
    defineProperty(t, k, d) { log.push("dst:define:" + String(k)); return Reflect.defineProperty(t, k, d); },
  });
}

function run(label: string, fn: () => string): void {
  log.length = 0;
  let out: string;
  try {
    out = fn();
  } catch (e: any) {
    out = "throw:" + e.constructor.name;
  }
  console.log(label + "=" + out + "|" + log.join(","));
}

run("source_only", () => JSON.stringify(Object.assign({}, tracedSource({ a: 1, b: 2 }))));
run("target_only", () => { const t: any = {}; Object.assign(tracedTarget(t), { x: 1, y: 2 }); return JSON.stringify(t); });
run("both", () => { const t: any = {}; Object.assign(tracedTarget(t), tracedSource({ a: 1 })); return JSON.stringify(t); });
run("two_sources", () => JSON.stringify(Object.assign({}, tracedSource({ a: 1 }), tracedSource({ b: 2 }))));
run("overlapping", () => JSON.stringify(Object.assign({ a: 0 }, tracedSource({ a: 1 }))));

// a non-enumerable descriptor stops the get entirely
run("nonenumerable", () => {
  const src: any = { visible: 1 };
  Object.defineProperty(src, "hidden", { value: 2, enumerable: false, configurable: true });
  return JSON.stringify(Object.assign({}, tracedSource(src)));
});
// and a lying descriptor trap is enough to hide an ordinary key
run("lying_descriptor", () => {
  const p: any = new Proxy({ a: 1, b: 2 }, {
    ownKeys(t) { log.push("src:ownKeys"); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) {
      log.push("src:gopd:" + String(k));
      const d = Reflect.getOwnPropertyDescriptor(t, k) as any;
      if (k === "a" && d) d.enumerable = false;
      return d;
    },
    get(t, k, r) { log.push("src:get:" + String(k)); return Reflect.get(t, k, r); },
  });
  return JSON.stringify(Object.assign({}, p));
});

// symbols are copied, in the ownKeys order the trap reports
const sym = Symbol("s");
const symSource: any = { a: 1 };
symSource[sym] = "S";
const symCopy: any = Object.assign({}, new Proxy(symSource, {}));
console.log("symbol_copied=" + symCopy[sym] + ",keys=" + Reflect.ownKeys(symCopy).map(String).join("|"));

// a refusing set trap: assign throws and the keys before it are already written
const partialTarget: any = {};
const partial: any = new Proxy(partialTarget, {
  set(t, k, v, r) { if (k === "b") return false; return Reflect.set(t, k, v, r); },
});
try {
  Object.assign(partial, { a: 1, b: 2, c: 3 });
  console.log("partial=ok");
} catch (e: any) {
  console.log("partial=throw:" + e.constructor.name);
}
console.log("partial_written=" + Object.keys(partialTarget).join("|"));

// a getter that throws on the source stops assign at that key
const throwingSource: any = new Proxy({ a: 1, get boom(): number { throw new RangeError("x"); }, c: 3 }, {});
const throwTarget: any = {};
try {
  Object.assign(throwTarget, throwingSource);
  console.log("throwing=ok");
} catch (e: any) {
  console.log("throwing=throw:" + e.constructor.name);
}
console.log("throwing_written=" + Object.keys(throwTarget).join("|"));

// assign returns the TARGET proxy itself, not the underlying object
const identityTarget: any = {};
const identityProxy: any = new Proxy(identityTarget, {});
console.log("returns_proxy=" + (Object.assign(identityProxy, { z: 1 }) === identityProxy));
console.log("returns_not_target=" + (Object.assign(identityProxy, { z: 1 }) === identityTarget));

// primitives as sources are wrapped, and null/undefined sources are skipped
console.log("string_source=" + JSON.stringify(Object.assign({}, new Proxy(new String("ab"), {}))));
console.log("nullish=" + JSON.stringify(Object.assign({ k: 1 }, null, undefined, new Proxy({ m: 2 }, {}))));

// an ownKeys trap that invents a key with no descriptor contributes nothing
const ghost: any = new Proxy({ real: 1 } as any, { ownKeys() { return ["real", "ghost"]; } });
console.log("ghost=" + JSON.stringify(Object.assign({}, ghost)));

// a proxy target that is FROZEN underneath: with no set trap the writes are
// refused one by one, and the first refusal throws
const frozenUnder: any = Object.freeze({ a: 1 });
try {
  Object.assign(new Proxy(frozenUnder, {}), { a: 2 });
  console.log("frozen_under=ok");
} catch (e: any) {
  console.log("frozen_under=throw:" + e.constructor.name);
}
console.log("frozen_under_value=" + frozenUnder.a);

// assigning a source ONTO ITSELF through a proxy is a no-op with the full trap
// sequence still performed
log.length = 0;
const selfTarget: any = { a: 1, b: 2 };
const self: any = new Proxy(selfTarget, {
  ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t, k) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
  set(t, k, v, r) { log.push("set:" + String(k)); return Reflect.set(t, k, v, r); },
});
Object.assign(self, self);
console.log("self_assign=" + JSON.stringify(selfTarget));
console.log("self_log=" + log.join(","));

// the target of assign is coerced with ToObject, so a proxy of a boxed value
// still collects the copied keys on the box
const boxed: any = new Proxy(new Number(3), {});
Object.assign(boxed, { extra: "E" });
console.log("boxed_extra=" + boxed.extra + ",keys=" + Object.keys(boxed).join("|"));
