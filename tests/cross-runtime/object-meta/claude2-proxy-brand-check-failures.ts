// Pins the limit of proxy transparency: a method that reads an INTERNAL SLOT
// rejects the proxy however faithfully the handler forwards, because the slot
// is on the target and `this` is the proxy. Object.prototype.toString shows the
// same split — the tag comes from @@toStringTag, which does forward.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const map = new Map<string, number>([["a", 1]]);
const set = new Set<number>([1, 2]);
const date = new Date(0);
const re = /ab+/g;
const promise = Promise.resolve(1);
const wm = new WeakMap<object, number>();
const key: any = {};
wm.set(key, 5);

// a fully forwarding handler — nothing here is a lie
const forward: any = {
  get(t: any, k: any, r: any) { return Reflect.get(t, k, r); },
  has(t: any, k: any) { return Reflect.has(t, k); },
};
const pMap: any = new Proxy(map, forward);
const pSet: any = new Proxy(set, forward);
const pDate: any = new Proxy(date, forward);
const pRe: any = new Proxy(re, forward);
const pProm: any = new Proxy(promise, forward);
const pWm: any = new Proxy(wm, forward);

attempt("map_get", () => String(pMap.get("a")));
attempt("map_size", () => String(pMap.size));
attempt("map_iterate", () => { let n = 0; for (const _e of pMap) n++; return String(n); });
attempt("set_has", () => String(pSet.has(1)));
attempt("date_getTime", () => String(pDate.getTime()));
attempt("date_toISOString", () => String(pDate.toISOString()));
attempt("regexp_test", () => String(pRe.test("abb")));
attempt("regexp_source", () => String(pRe.source));
attempt("promise_then", () => String(typeof pProm.then(() => 1)));
attempt("weakmap_get", () => String(pWm.get(key)));

// the same methods applied to the TARGET through the proxy's own function work
console.log("map_via_target=" + Reflect.apply(pMap.get, map, ["a"]));
console.log("date_via_target=" + Reflect.apply(pDate.getTime, date, []));
console.log("set_via_target=" + Reflect.apply(pSet.has, set, [2]));

// the brand check is on `this`, so a get trap that binds the target repairs it
const bound: any = new Proxy(map, {
  get(t: any, k: any, r: any) {
    const v = Reflect.get(t, k, r);
    return typeof v === "function" ? v.bind(t) : v;
  },
});
console.log("bound_get=" + bound.get("a"));
attempt("bound_size", () => String(bound.size));

// instanceof and the prototype chain are unaffected: those never touch a slot
console.log("instanceof=" + (pMap instanceof Map) + ":" + (pDate instanceof Date) + ":" + (pRe instanceof RegExp));
console.log("proto=" + (Object.getPrototypeOf(pMap) === Map.prototype));
console.log("has_method=" + ("get" in pMap) + ":" + (typeof pMap.get));

// Object.prototype.toString reads @@toStringTag through the get trap, so the
// classes that define one keep their tag and the ones that do not fall back
console.log("tag_map=" + Object.prototype.toString.call(pMap));
console.log("tag_set=" + Object.prototype.toString.call(pSet));
console.log("tag_promise=" + Object.prototype.toString.call(pProm));
console.log("tag_weakmap=" + Object.prototype.toString.call(pWm));
console.log("tag_date=" + Object.prototype.toString.call(pDate));
console.log("tag_regexp=" + Object.prototype.toString.call(pRe));
console.log("tag_error=" + Object.prototype.toString.call(new Proxy(new Error("x"), forward)));
console.log("tag_arguments=" + Object.prototype.toString.call(new Proxy((function () { return arguments; })(), forward)));

// a get trap may invent the tag for anything
console.log("tag_invented=" + Object.prototype.toString.call(new Proxy({}, { get(_t, k) { return k === Symbol.toStringTag ? "Invented" : undefined; } })));
console.log("tag_nonstring=" + Object.prototype.toString.call(new Proxy({}, { get(_t, k) { return k === Symbol.toStringTag ? 42 : undefined; } })));

// a boxed primitive behind a proxy loses valueOf's slot too
attempt("boxed_valueOf", () => String((new Proxy(new Number(7), forward) as any).valueOf()));
attempt("boxed_coerce", () => String(Number(new Proxy(new Number(7), forward))));
console.log("string_boxed_index=" + (new Proxy(new String("hi"), forward) as any)[0]);
