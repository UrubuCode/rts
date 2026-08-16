// Cross-runtime: a Proxy wrapping a Map or a Set. The proxy forwards the METHOD
// but not the internal slot, so calling it with the proxy as `this` fails the
// brand check — the wrapper only works when the trap re-binds the receiver.

const target = new Map<any, any>([["a", 1], ["b", 2]]);
const traps: string[] = [];

const naive: any = new Proxy(target, {
  get(t: any, k: any, r: any) { traps.push("get:" + String(k)); return Reflect.get(t, k, r); },
  has(t: any, k: any) { traps.push("has:" + String(k)); return Reflect.has(t, k); },
  ownKeys(t: any) { traps.push("ownKeys"); return Reflect.ownKeys(t); },
  getPrototypeOf(t: any) { traps.push("getProto"); return Reflect.getPrototypeOf(t); },
});

function probe(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}

// --- every data method fails: `this` is the proxy, which has no [[MapData]] ---
traps.length = 0;
probe("naive_get", () => naive.get("a"));
probe("naive_set", () => naive.set("c", 3));
probe("naive_has", () => naive.has("a"));
probe("naive_delete", () => naive.delete("a"));
probe("naive_size", () => naive.size);
probe("naive_forEach", () => naive.forEach(() => { /* never runs */ }));
probe("naive_spread", () => [...naive].length);
probe("naive_entries", () => naive.entries().next().value);
console.log("naive_traps=" + traps.join("|"));
console.log("target_untouched=" + target.size + ":" + [...target.keys()].join(","));

// --- the prototype-level identity checks still pass through the proxy ---
console.log("instanceof_map=" + (naive instanceof Map));
console.log("proto_is_map_proto=" + (Object.getPrototypeOf(naive) === Map.prototype));
console.log("tostring_tag=" + Object.prototype.toString.call(naive));
console.log("get_in_proxy=" + ("get" in naive));
console.log("typeof=" + typeof naive);

// --- calling the prototype method directly ON the proxy fails the same way ---
probe("proto_get_on_proxy", () => Map.prototype.get.call(naive, "a"));
probe("proto_size_on_proxy", () => (Object.getOwnPropertyDescriptor(Map.prototype, "size") as any).get.call(naive));

// --- a trap that BINDS the method makes the wrapper work ---
const bound: any = new Proxy(target, {
  get(t: any, k: any) {
    const v = Reflect.get(t, k);
    return typeof v === "function" ? v.bind(t) : v;
  },
});
console.log("bound_get=" + bound.get("a"));
console.log("bound_size=" + bound.size);
console.log("bound_has=" + bound.has("b"));
console.log("bound_spread=" + [...bound].map((e: any) => e.join(":")).join(","));
console.log("bound_set_returns_target=" + (bound.set("c", 3) === target));
console.log("bound_after_set=" + [...target.keys()].join(","));
target.delete("c");

// --- a Set behind a proxy behaves identically ---
const sTarget = new Set([1, 2]);
const sProxy: any = new Proxy(sTarget, {});
probe("set_add_via_proxy", () => sProxy.add(3));
probe("set_size_via_proxy", () => sProxy.size);
console.log("set_target_size=" + sTarget.size);
console.log("set_instanceof=" + (sProxy instanceof Set));

// --- a proxy is an ordinary object as a KEY: identity is the proxy's own ---
const keyMap = new Map<any, string>();
keyMap.set(target, "target");
keyMap.set(naive, "proxy");
console.log("key_count=" + keyMap.size);
console.log("key_target=" + keyMap.get(target));
console.log("key_proxy=" + keyMap.get(naive));
console.log("key_distinct=" + (keyMap.get(target) === keyMap.get(naive)));

const keySet = new Set([target, naive, target]);
console.log("set_of_proxy_and_target=" + keySet.size);

// --- a weak collection accepts a proxy and fires no trap doing it ---
traps.length = 0;
const wm = new WeakMap<any, string>();
wm.set(naive, "weak");
console.log("weakmap_get=" + wm.get(naive));
console.log("weakmap_target_missing=" + String(wm.get(target)));
console.log("weakmap_traps=" + traps.length);

// --- a proxy of a Map is not set-like enough for the Set operations ---
probe("union_with_map_proxy", () => (new Set([1]) as any).union(naive));
probe("union_with_bound_proxy", () => "{" + [...(new Set([1]) as any).union(bound)].join(",") + "}");

// --- a revoked proxy is still an object, and still a usable weak/map key ---
const rev = Proxy.revocable({ tag: "rev" }, {});
const revMap = new Map([[rev.proxy, "alive"]]);
console.log("revocable_before=" + revMap.get(rev.proxy));
rev.revoke();
console.log("revoked_typeof=" + typeof rev.proxy);
console.log("revoked_still_key=" + revMap.get(rev.proxy));
console.log("revoked_in_set=" + new Set([rev.proxy, rev.proxy]).size);
const revWeak = new WeakSet([rev.proxy]);
console.log("revoked_in_weakset=" + revWeak.has(rev.proxy));
probe("revoked_property_read", () => (rev.proxy as any).tag);
