// Cross-runtime: lastIndex is an OWN data property of every regex — writable,
// not enumerable, NOT configurable — and that shape has a consequence nothing in
// the corpus pins: making it non-writable turns `exec` on a /g regex into a
// TypeError, because the spec's Set is performed with Throw=true. The existing
// lastIndex fixtures pin the VALUES; this one pins the property itself.
// Reflect.defineProperty/Reflect.set are used so the file reads the same in
// sloppy and strict mode.

function desc(o: any, k: string): string {
  const d: any = Object.getOwnPropertyDescriptor(o, k);
  if (d === undefined) return "none";
  if ("value" in d) return "w=" + d.writable + " e=" + d.enumerable + " c=" + d.configurable;
  return "accessor e=" + d.enumerable + " c=" + d.configurable;
}

function attempt(f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- the descriptor, on a fresh regex of each shape ---
console.log("literal=" + desc(/a/, "lastIndex"));
console.log("global=" + desc(/a/g, "lastIndex"));
console.log("sticky=" + desc(/a/y, "lastIndex"));
console.log("ctor=" + desc(new RegExp("a"), "lastIndex"));
console.log("own=" + Object.prototype.hasOwnProperty.call(/a/, "lastIndex"));
console.log("on-proto=" + Object.prototype.hasOwnProperty.call(RegExp.prototype, "lastIndex"));
console.log("keys=" + Object.keys(/a/g).join(","));
console.log("ownNames=" + Object.getOwnPropertyNames(/a/g).join(","));
console.log("initial=" + /a/g.lastIndex);

// --- a subclass instance gets the same own property, not an inherited one ---
class Sub extends RegExp {}
const sub = new Sub("a", "g");
console.log("sub-own=" + Object.prototype.hasOwnProperty.call(sub, "lastIndex"));
console.log("sub-desc=" + desc(sub, "lastIndex"));

// --- it is writable, and accepts any value: no coercion happens on write ---
const w = /a/g;
console.log("set-num=" + Reflect.set(w, "lastIndex", 3) + "/" + w.lastIndex);
console.log("set-str=" + Reflect.set(w, "lastIndex", "1") + "/" + JSON.stringify(w.lastIndex));
console.log("set-obj=" + Reflect.set(w, "lastIndex", {}) + "/" + typeof w.lastIndex);
console.log("set-neg=" + Reflect.set(w, "lastIndex", -5) + "/" + w.lastIndex);
w.lastIndex = 0;

// --- the coercion happens on READ, inside exec: ToLength of a string index ---
const r = /a/g;
r.lastIndex = "1" as any;
console.log("str-index-exec=" + JSON.stringify(r.exec("aaa")));
console.log("str-index-after=" + r.lastIndex + "/" + typeof r.lastIndex);
const r2 = /a/g;
r2.lastIndex = -1 as any;
console.log("neg-index-exec=" + JSON.stringify(r2.exec("aaa")) + "/" + r2.lastIndex);
const r3 = /a/g;
r3.lastIndex = 2.7 as any;
console.log("frac-index-exec=" + JSON.stringify(r3.exec("aaa")) + "/" + r3.lastIndex);
const r4 = /a/g;
r4.lastIndex = NaN as any;
console.log("nan-index-exec=" + JSON.stringify(r4.exec("aaa")) + "/" + r4.lastIndex);

// --- it is NOT configurable: it cannot be deleted or turned into an accessor ---
const c = /a/g;
console.log("delete=" + Reflect.deleteProperty(c, "lastIndex"));
console.log("still-there=" + Object.prototype.hasOwnProperty.call(c, "lastIndex"));
console.log("to-accessor=" + Reflect.defineProperty(c, "lastIndex", { get: () => 0 }));
console.log("to-enumerable=" + Reflect.defineProperty(c, "lastIndex", { enumerable: true }));
console.log("to-configurable=" + Reflect.defineProperty(c, "lastIndex", { configurable: true }));
console.log("after-attempts=" + desc(c, "lastIndex"));

// --- but writable:true -> false IS allowed, once and irreversibly ---
const frozen = /a/g;
frozen.lastIndex = 0;
console.log("freeze=" + Reflect.defineProperty(frozen, "lastIndex", { writable: false }));
console.log("frozen-desc=" + desc(frozen, "lastIndex"));
console.log("unfreeze=" + Reflect.defineProperty(frozen, "lastIndex", { writable: true }));
console.log("reflect-set=" + Reflect.set(frozen, "lastIndex", 2) + "/" + frozen.lastIndex);

// --- and THAT is what makes a /g exec throw, in either mode ---
console.log("g-exec=" + attempt(() => frozen.exec("a")));
console.log("g-test=" + attempt(() => frozen.test("a")));
console.log("g-replace=" + attempt(() => "a".replace(frozen, "-")));
console.log("g-match=" + attempt(() => "a".match(frozen)));

// --- a sticky regex writes lastIndex too, so it throws the same way ---
const stickyFrozen = /a/y;
Reflect.defineProperty(stickyFrozen, "lastIndex", { writable: false });
console.log("y-exec=" + attempt(() => stickyFrozen.exec("a")));

// --- a PLAIN regex never writes lastIndex, so a frozen one still works ---
const plainFrozen = /a/;
Reflect.defineProperty(plainFrozen, "lastIndex", { writable: false });
console.log("plain-exec=" + attempt(() => JSON.stringify(plainFrozen.exec("xa"))));
console.log("plain-test=" + attempt(() => plainFrozen.test("xa")));
console.log("plain-lastIndex=" + plainFrozen.lastIndex);

// --- a failing /g match must still write 0, so a frozen one throws on failure ---
const failFrozen = /zz/g;
Reflect.defineProperty(failFrozen, "lastIndex", { writable: false });
console.log("g-fail=" + attempt(() => failFrozen.exec("a")));

// --- Object.freeze on the whole regex is the same story ---
const whole = /a/g;
Object.freeze(whole);
console.log("frozen-obj=" + Object.isFrozen(whole));
console.log("frozen-obj-exec=" + attempt(() => whole.exec("a")));
console.log("frozen-obj-plain=" + attempt(() => JSON.stringify(Object.freeze(/a/).exec("a"))));
