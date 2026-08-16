// Cross-runtime: the weak APIs deal in raw object IDENTITY, so every exotic
// object is an ordinary target — a Proxy, a revoked Proxy, a bound function, a
// class constructor — and holding one fires no trap, because nothing about the
// object is ever read. Nothing here asserts a collection happened.

const traps: string[] = [];
const targetObj: any = { id: "target" };
const proxy: any = new Proxy(targetObj, {
  get(t: any, k: any, r: any) { traps.push("get:" + String(k)); return Reflect.get(t, k, r); },
  has(t: any, k: any) { traps.push("has:" + String(k)); return Reflect.has(t, k); },
  ownKeys(t: any) { traps.push("ownKeys"); return Reflect.ownKeys(t); },
  getPrototypeOf(t: any) { traps.push("getProto"); return Reflect.getPrototypeOf(t); },
  getOwnPropertyDescriptor(t: any, k: any) { traps.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
});

// --- a Proxy is a valid target for all five weak surfaces ---
const wm = new WeakMap<any, string>();
const ws = new WeakSet<any>();
const ref = new WeakRef(proxy);
const fr = new FinalizationRegistry(() => { /* never observed here */ });

wm.set(proxy, "via_proxy");
ws.add(proxy);
console.log("weakmap_get=" + wm.get(proxy));
console.log("weakset_has=" + ws.has(proxy));
console.log("weakref_deref_is_proxy=" + (ref.deref() === proxy));
console.log("register=" + String(fr.register(proxy, "held", proxy)));
console.log("unregister=" + fr.unregister(proxy));

// --- and not one trap ran while doing it ---
console.log("traps=" + JSON.stringify(traps.join("|")));

// --- the proxy and its target are two different identities ---
console.log("target_not_key=" + wm.has(targetObj) + ":" + ws.has(targetObj));
console.log("deref_not_target=" + (ref.deref() === targetObj));
wm.set(targetObj, "via_target");
console.log("both_held=" + wm.get(proxy) + ":" + wm.get(targetObj));
console.log("traps_after_target=" + JSON.stringify(traps.join("|")));

// --- a REVOKED proxy is still an object, and still a working key ---
const rev = Proxy.revocable({ id: "revocable" }, {});
const revWm = new WeakMap<any, string>();
revWm.set(rev.proxy, "before");
const revRef = new WeakRef(rev.proxy);
rev.revoke();
console.log("revoked_typeof=" + typeof rev.proxy);
console.log("revoked_weakmap_get=" + revWm.get(rev.proxy));
console.log("revoked_weakmap_has=" + revWm.has(rev.proxy));
console.log("revoked_deref_same=" + (revRef.deref() === rev.proxy));
console.log("revoked_set_new=" + (revWm.set(rev.proxy, "after") === revWm) + ":" + revWm.get(rev.proxy));
console.log("revoked_weakset=" + new WeakSet([rev.proxy]).has(rev.proxy));
console.log("revoked_register=" + String(new FinalizationRegistry(() => { /* unused */ }).register(rev.proxy, 1)));
console.log("revoked_new_weakref=" + (typeof new WeakRef(rev.proxy).deref()));
try { void (rev.proxy as any).id; console.log("revoked_read=no_throw"); }
catch (e: any) { console.log("revoked_read=" + e.constructor.name); }
console.log("revoked_delete=" + revWm.delete(rev.proxy) + ":" + revWm.has(rev.proxy));

// --- every callable kind is a target of its own identity ---
function plain(): number { return 1; }
const bound = plain.bind(null);
const arrow = () => 1;
class Klass { static tag = "k"; }
function* genFn(): any { yield 1; }
async function asyncFn(): Promise<void> { /* shape only */ }
const callables: any[] = [plain, bound, arrow, Klass, genFn, asyncFn, Math.max, Symbol];
const fnMap = new WeakMap<any, number>();
callables.forEach((f, i) => fnMap.set(f, i));
console.log("callables_held=" + callables.map((f) => String(fnMap.get(f))).join(","));
console.log("bound_is_distinct=" + (bound === plain) + ":" + fnMap.get(bound) + "!=" + fnMap.get(plain));
console.log("second_bind_distinct=" + (plain.bind(null) === bound) + ":" + fnMap.has(plain.bind(null)));

// --- exotic non-callables too ---
const nullProto: any = Object.create(null);
const argumentsObj: any = (function () { return arguments; })(1);
const frozen: any = Object.freeze({ f: 1 });
const sealed: any = Object.seal({ s: 1 });
const arr: any = [1, 2];
const typed: any = new Uint8Array(2);
const err: any = new TypeError("x");
const gen: any = genFn();
const promise: any = Promise.resolve(1);
const boxed: any = new Number(1);
const exotics: any[] = [nullProto, argumentsObj, frozen, sealed, arr, typed, err, gen, promise, boxed, Math, JSON, globalThis];
const exoticSet = new WeakSet<any>(exotics);
console.log("exotics_all_held=" + exotics.every((o) => exoticSet.has(o)));
console.log("exotics_count_distinct=" + exotics.length);
console.log("frozen_as_weakmap_key=" + (function () { const w = new WeakMap(); w.set(frozen, "ok"); return w.get(frozen); })());
console.log("null_proto_deref=" + (new WeakRef(nullProto).deref() === nullProto));
console.log("globalThis_deref=" + (new WeakRef(globalThis as any).deref() === globalThis));

// --- a WeakRef and a FinalizationRegistry are themselves ordinary objects ---
console.log("weakref_as_key=" + (function () { const w = new WeakMap(); w.set(ref, "held"); return w.get(ref); })());
console.log("registry_as_key=" + (function () { const w = new WeakSet(); w.add(fr); return w.has(fr); })());
console.log("weakref_of_weakref=" + (new WeakRef(ref).deref() === ref));
console.log("weakmap_of_weakmap=" + (function () { const outer = new WeakMap(); outer.set(wm, "inner"); return outer.get(wm); })());

// --- a symbol target: unique yes, registered no, across all four APIs ---
const uniq: any = Symbol("exotic");
const reg: any = Symbol.for("claude2-exotic-registered");
function attempt(label: string, fn: () => any): void {
  try { console.log(label + "=ok:" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}
attempt("unique_weakset", () => new WeakSet([uniq]).has(uniq));
attempt("unique_weakref", () => new WeakRef(uniq).deref() === uniq);
attempt("unique_register", () => new FinalizationRegistry(() => { /* unused */ }).register(uniq, 1));
attempt("registered_weakset", () => new WeakSet([reg]).has(reg));
attempt("registered_weakref", () => new WeakRef(reg).deref() === reg);
attempt("registered_register", () => new FinalizationRegistry(() => { /* unused */ }).register(reg, 1));
attempt("wellknown_weakref", () => new WeakRef(Symbol.iterator as any).deref() === Symbol.iterator);

// --- while a strong reference is held, deref keeps answering the same object ---
const strong: any = { n: 0 };
const stable = new WeakRef(strong);
for (let i = 0; i < 5; i++) strong.n = i;
console.log("stable_identity=" + (stable.deref() === strong) + ":" + (stable.deref() as any).n);
console.log("stable_repeat=" + (stable.deref() === stable.deref()));
console.log("stable_typeof=" + typeof stable.deref());
