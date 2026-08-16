// Pins for-in over a proxy chain: each level answers ownKeys once, every key is
// filtered by its own getOwnPropertyDescriptor, a key already seen lower down
// is skipped even when it was NOT enumerable there, and symbols never appear.

const log: string[] = [];

function level(target: any, tag: string, keys?: any[]): any {
  return new Proxy(target, {
    ownKeys(t) { log.push("ownKeys" + tag); return keys === undefined ? Reflect.ownKeys(t) : keys; },
    getOwnPropertyDescriptor(t, k) {
      log.push("gopd" + tag + ":" + String(k));
      const d = Reflect.getOwnPropertyDescriptor(t, k);
      if (d === undefined && keys !== undefined) return { value: "V" + tag, enumerable: true, configurable: true, writable: true };
      return d;
    },
    getPrototypeOf(t) { log.push("proto" + tag); return Reflect.getPrototypeOf(t); },
    get(t, k, r) { log.push("get" + tag + ":" + String(k)); return Reflect.get(t, k, r); },
  });
}

function names(o: any): string {
  const acc: string[] = [];
  for (const k in o) acc.push(k);
  return acc.join("|");
}

// one level, mixed enumerability
const flatTarget: any = { a: 1, b: 2 };
Object.defineProperty(flatTarget, "hidden", { value: 3, enumerable: false, configurable: true });
(flatTarget as any)[Symbol("s")] = 4;
log.length = 0;
const flat = level(flatTarget, "1");
console.log("flat=" + names(flat));
console.log("flat_log=" + log.join(","));

// two proxy levels: the child's keys, then the prototype's, deduped
const protoTarget: any = { shared: "P", onlyProto: "P2" };
const childTarget: any = { shared: "C", onlyChild: "C2" };
const protoLevel = level(protoTarget, "P");
Object.setPrototypeOf(childTarget, protoLevel);
const childLevel = level(childTarget, "C");
log.length = 0;
console.log("chain=" + names(childLevel));
console.log("chain_log=" + log.join(","));

// a NON-enumerable own key on the child still shadows an enumerable one above
const shadowProto: any = { k: "P" };
const shadowChild: any = Object.create(shadowProto);
Object.defineProperty(shadowChild, "k", { value: "C", enumerable: false, configurable: true });
shadowChild.visible = 1;
console.log("shadow_direct=" + names(shadowChild));
console.log("shadow_proxy=" + names(new Proxy(shadowChild, {})));

// an ownKeys trap inventing keys: each invented key needs a descriptor to be
// visited, and Object.keys agrees with for-in
log.length = 0;
const invented = level({}, "I", ["x", "y"]);
console.log("invented_forin=" + names(invented));
console.log("invented_keys=" + Object.keys(invented).join("|"));
console.log("invented_log=" + log.join(","));

// a descriptor trap reporting enumerable:false hides the key from both
const hiding: any = new Proxy({ a: 1, b: 2 }, {
  getOwnPropertyDescriptor(t, k) {
    const d = Reflect.getOwnPropertyDescriptor(t, k) as any;
    if (k === "a" && d) d.enumerable = false;
    return d;
  },
});
console.log("hiding_forin=" + names(hiding));
console.log("hiding_keys=" + Object.keys(hiding).join("|"));
console.log("hiding_in=" + ("a" in hiding));
console.log("hiding_value=" + hiding.a);

// integer-like keys come out of the trap in the trap's order for a proxy
const ordered: any = new Proxy({} as any, {
  ownKeys() { return ["10", "b", "2", "a"]; },
  getOwnPropertyDescriptor() { return { value: 1, enumerable: true, configurable: true, writable: true }; },
});
console.log("order_forin=" + names(ordered));
console.log("order_keys=" + Object.keys(ordered).join("|"));
console.log("order_ownKeys=" + Reflect.ownKeys(ordered).join("|"));

// symbols reported by ownKeys are filtered out of for-in without a descriptor
// call, so the log shows no gopd for them
const symKey = Symbol("hidden");
const symLog: string[] = [];
const withSymbols: any = new Proxy({} as any, {
  ownKeys() { return ["s", symKey]; },
  getOwnPropertyDescriptor(_t, k) { symLog.push(String(k)); return { value: 1, enumerable: true, configurable: true, writable: true }; },
});
console.log("sym_forin=" + names(withSymbols));
console.log("sym_gopd_calls=" + symLog.join(","));

// a proxy PROTOTYPE below an ordinary object: the walk reaches it through the
// ordinary object's [[GetPrototypeOf]], and its ownKeys trap answers once
const belowTarget: any = { fromProxy: 1, alsoShadowed: "P" };
const below: any = new Proxy(belowTarget, { ownKeys(t) { return Reflect.ownKeys(t); } });
const above: any = Object.create(below);
above.own = 1;
above.alsoShadowed = "C";
console.log("below_forin=" + names(above));
console.log("below_keys=" + Object.keys(above).join("|"));
console.log("below_value=" + above.alsoShadowed + ":" + above.fromProxy);

// a getPrototypeOf trap that ends the chain early hides the rest of it
const cut: any = new Proxy(Object.create({ hiddenAbove: 1 }, { visible: { value: 2, enumerable: true, configurable: true } }), {
  getPrototypeOf() { return null; },
});
console.log("cut_forin=" + names(cut));
console.log("cut_read=" + String(cut.hiddenAbove));

// a for-in that deletes the remaining keys mid-loop still terminates
const mutating: any = new Proxy({ a: 1, b: 2, c: 3 } as any, {});
const visited: string[] = [];
for (const k in mutating) {
  visited.push(k);
  Reflect.deleteProperty(mutating, "c");
}
console.log("mutating=" + visited.join("|"));
