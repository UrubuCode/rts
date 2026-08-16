// Cross-runtime: which PROXY TRAPS JSON.stringify fires, and in what order —
// isArray pierces the proxy, an object goes through ownKeys +
// getOwnPropertyDescriptor + get, and an array goes through length + index gets.

// --- a proxied plain object: the trap sequence ---
const log: string[] = [];
const objTarget = { a: 1, b: 2 };
const objProxy = new Proxy(objTarget, {
  ownKeys(t: any) { log.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t: any, k: any) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  get(t: any, k: any, r: any) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
  has(t: any, k: any) { log.push("has:" + String(k)); return Reflect.has(t, k); },
});
console.log("obj_out=" + JSON.stringify(objProxy));
console.log("obj_traps=" + log.join("|"));

// --- a proxied array: isArray sees through, so it serialises as an array ---
const arrLog: string[] = [];
const arrTarget = [10, 20];
const arrProxy = new Proxy(arrTarget, {
  get(t: any, k: any, r: any) { arrLog.push("get:" + String(k)); return Reflect.get(t, k, r); },
  ownKeys(t: any) { arrLog.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t: any, k: any) { arrLog.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
});
console.log("arr_isArray=" + Array.isArray(arrProxy));
console.log("arr_out=" + JSON.stringify(arrProxy));
console.log("arr_traps=" + arrLog.join("|"));

// --- a get trap that rewrites values ---
const rewritten = new Proxy({ a: 1, b: 2 }, {
  get(t: any, k: any) { return typeof t[k] === "number" ? t[k] * 100 : t[k]; },
});
console.log("rewritten=" + JSON.stringify(rewritten));

// --- ownKeys deciding what is serialised (the descriptor must agree) ---
const filtered = new Proxy({ a: 1, b: 2, c: 3 }, {
  ownKeys() { return ["a", "c"]; },
});
console.log("filtered=" + JSON.stringify(filtered));

// --- a key reported by ownKeys but non-enumerable is skipped ---
const nonEnum = new Proxy({ a: 1, b: 2 }, {
  getOwnPropertyDescriptor(t: any, k: any) {
    const d: any = Reflect.getOwnPropertyDescriptor(t, k);
    if (k === "b" && d) d.enumerable = false;
    return d;
  },
});
console.log("non_enumerable=" + JSON.stringify(nonEnum));

// --- toJSON is looked up through the get trap ---
const tjLog: string[] = [];
const withToJSON = new Proxy({ a: 1 }, {
  get(t: any, k: any, r: any) {
    tjLog.push(String(k));
    if (k === "toJSON") return function () { return "PROXY_JSON"; };
    return Reflect.get(t, k, r);
  },
});
console.log("tojson_out=" + JSON.stringify(withToJSON));
console.log("tojson_gets=" + tjLog.join(","));

// --- a proxy nested inside a plain object ---
console.log("nested=" + JSON.stringify({ p: objProxy, n: 5 }));

// --- a proxy WRAPPING a proxy ---
const doubled = new Proxy(new Proxy({ x: 1 }, {}), {});
console.log("double_proxy=" + JSON.stringify(doubled));

// --- a proxy of a Date reaches Date.prototype.toJSON, but the proxy itself has
//     no [[DateValue]] slot, so the coercion inside it is refused ---
const dateProxy = new Proxy(new Date(Date.UTC(2021, 4, 6, 7, 8, 9)), {});
console.log("date_proxy_tojson_type=" + typeof (dateProxy as any).toJSON);
try { JSON.stringify(dateProxy); console.log("date_proxy=no_throw"); }
catch (e: any) { console.log("date_proxy=" + e.constructor.name); }

// --- a proxy whose get trap throws ---
const boom = new Proxy({ a: 1 }, { get() { throw new RangeError("trap"); } });
try { JSON.stringify(boom); console.log("throwing_trap=no_throw"); }
catch (e: any) { console.log("throwing_trap=" + e.constructor.name); }

// --- a REVOKED proxy is a TypeError at the first trap ---
const rev = Proxy.revocable({ a: 1 }, {});
console.log("before_revoke=" + JSON.stringify(rev.proxy));
rev.revoke();
try { JSON.stringify(rev.proxy); console.log("after_revoke=no_throw"); }
catch (e: any) { console.log("after_revoke=" + e.constructor.name); }

// --- a proxy of a function serialises like a function: elided / null ---
const fnProxy = new Proxy(function () { /* fn */ }, {});
console.log("fn_proxy_typeof=" + typeof fnProxy);
console.log("fn_proxy_in_obj=" + JSON.stringify({ f: fnProxy }));
console.log("fn_proxy_in_arr=" + JSON.stringify([fnProxy]));
console.log("fn_proxy_top=" + String(JSON.stringify(fnProxy)));

// --- a proxy in a cycle is still detected ---
const cyc: any = { a: 1 };
const cycProxy: any = new Proxy(cyc, {});
cyc.self = cycProxy;
try { JSON.stringify(cycProxy); console.log("proxy_cycle=no_throw"); }
catch (e: any) { console.log("proxy_cycle=" + e.constructor.name); }
