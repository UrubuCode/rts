// Pins the [[OwnPropertyKeys]] invariants a Proxy cannot break: no duplicate
// keys, no non-key entries, every non-configurable own key must be reported,
// and a non-extensible target must be reported EXACTLY. 98/codex_ownkeys cover
// the happy path and the ordering only.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

// a free target: the trap may invent keys and reorder freely
const free = new Proxy({ a: 1, b: 2 }, { ownKeys() { return ["b", "invented", "a"]; } });
attempt("free", () => Reflect.ownKeys(free).join("|"));
attempt("free_names", () => Object.getOwnPropertyNames(free).join("|"));
// Object.keys filters by the descriptor, and "invented" has none on the target
attempt("free_keys", () => Object.keys(free).join("|"));

// duplicates are rejected
const dup = new Proxy({ a: 1 }, { ownKeys() { return ["a", "a"]; } });
attempt("dup", () => Reflect.ownKeys(dup).join("|"));

// a non-key entry is rejected
const bad = new Proxy({}, { ownKeys() { return [1 as any]; } });
attempt("nonkey", () => Reflect.ownKeys(bad).join("|"));

// a non-object trap result is rejected
const notlist = new Proxy({}, { ownKeys() { return "ab" as any; } });
attempt("notlist", () => Reflect.ownKeys(notlist).join("|"));

// a non-configurable own key MUST be reported
const nc: any = {};
Object.defineProperty(nc, "fixed", { value: 1, configurable: false, enumerable: true });
nc.loose = 2;
const hideNc = new Proxy(nc, { ownKeys() { return ["loose"]; } });
attempt("hide_nonconf", () => Reflect.ownKeys(hideNc).join("|"));
const keepNc = new Proxy(nc, { ownKeys() { return ["fixed"]; } });
attempt("hide_conf_only", () => Reflect.ownKeys(keepNc).join("|"));

// a non-extensible target must be reported exactly (extra key)
const sealedT: any = { x: 1, y: 2 };
Object.preventExtensions(sealedT);
const extra = new Proxy(sealedT, { ownKeys() { return ["x", "y", "z"]; } });
attempt("nonext_extra", () => Reflect.ownKeys(extra).join("|"));
const missing = new Proxy(sealedT, { ownKeys() { return ["x"]; } });
attempt("nonext_missing", () => Reflect.ownKeys(missing).join("|"));
const reordered = new Proxy(sealedT, { ownKeys() { return ["y", "x"]; } });
attempt("nonext_reorder", () => Reflect.ownKeys(reordered).join("|"));

// symbols travel through the trap and are split out by the two Object statics
const s1 = Symbol("s1");
const s2 = Symbol("s2");
const symProxy = new Proxy({}, { ownKeys() { return ["a", s2, "b", s1]; } });
attempt("sym_all", () => Reflect.ownKeys(symProxy).map(String).join("|"));
attempt("sym_names", () => Object.getOwnPropertyNames(symProxy).join("|"));
attempt("sym_symbols", () => Object.getOwnPropertySymbols(symProxy).map(String).join("|"));

// the trap result order is preserved verbatim: no integer-key sorting
const unsorted = new Proxy({}, {
  ownKeys() { return ["b", "10", "2", "a", "1"]; },
  getOwnPropertyDescriptor() { return { value: 1, enumerable: true, configurable: true, writable: true }; },
});
attempt("order_raw", () => Reflect.ownKeys(unsorted).join("|"));
attempt("order_keys", () => Object.keys(unsorted).join("|"));
attempt("order_forin", () => {
  const out: string[] = [];
  for (const k in unsorted) out.push(k);
  return out.join("|");
});
attempt("order_json", () => JSON.stringify(unsorted));
attempt("order_assign", () => Object.keys(Object.assign({}, unsorted)).join("|"));
attempt("order_spread", () => Object.keys({ ...(unsorted as any) }).join("|"));

// missing trap falls through to the target
const passthrough = new Proxy({ q: 1, 2: "two" }, {});
attempt("passthrough", () => Reflect.ownKeys(passthrough).join("|"));

// the trap is called exactly once per operation
let calls = 0;
const counted = new Proxy({ a: 1, b: 2 }, {
  ownKeys(t) { calls++; return Reflect.ownKeys(t); },
});
Object.keys(counted);
console.log("calls_keys=" + calls);
calls = 0;
JSON.stringify(counted);
console.log("calls_json=" + calls);
