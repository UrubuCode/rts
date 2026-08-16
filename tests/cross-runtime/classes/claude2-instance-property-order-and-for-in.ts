// Cross-runtime: own-key ORDER on a class instance — integer-like keys first in
// ascending numeric order, then string keys in creation order, then symbols —
// and what for-in adds on top of it by walking the prototype chain.
class Mixed {
  b: string = "b";
  a: string = "a";

  constructor() {
    (this as any)["10"] = "ten";
    (this as any)["2"] = "two";
    (this as any).z = "z";
    (this as any)["-1"] = "neg";
    (this as any)["01"] = "leading-zero";
    (this as any)["1.5"] = "frac";
    (this as any)[0] = "zero";
    (this as any)[Symbol.for("s1")] = "sym";
  }

  method(): string {
    return "m";
  }
}

const m: any = new Mixed();
console.log("keys=" + Object.keys(m).join(","));
console.log("names=" + Object.getOwnPropertyNames(m).join(","));
console.log("symbols=" + Object.getOwnPropertySymbols(m).length);
console.log("entries=" + Object.entries(m).map((p) => p[0] + ":" + p[1]).join("|"));
console.log("json=" + JSON.stringify(m));
console.log("assign=" + Object.keys(Object.assign({}, m)).join(","));
console.log("spread=" + Object.keys({ ...m }).join(","));
console.log("reflect-ownkeys-strings=" + Reflect.ownKeys(m).filter((k) => typeof k === "string").join(","));

// Deleting and re-adding moves a string key to the end; an integer key keeps
// its numeric slot.
delete m.a;
m.a = "a-again";
delete m["2"];
(m as any)["2"] = "two-again";
console.log("after-redo=" + Object.keys(m).join(","));

// for-in adds enumerable inherited properties. Class methods are
// non-enumerable, so nothing from the class body shows up.
const forIn: string[] = [];
for (const k in m) {
  forIn.push(k);
}
console.log("for-in=" + forIn.join(","));
console.log("for-in-has-method=" + (forIn.indexOf("method") >= 0));

// An enumerable property ASSIGNED onto the prototype does show up, after the
// own keys, and hasOwnProperty separates the two.
(Mixed.prototype as any).inherited = "from-proto";
const forIn2: string[] = [];
const owned: string[] = [];
for (const k in m) {
  forIn2.push(k);
  if (Object.prototype.hasOwnProperty.call(m, k)) {
    owned.push(k);
  }
}
console.log("for-in2=" + forIn2.join(","));
console.log("for-in2-own=" + owned.join(","));
console.log("keys-unchanged=" + Object.keys(m).join(","));

// A shadowing own key is visited once, at its own position.
(m as any).inherited = "own-wins";
const forIn3: string[] = [];
for (const k in m) {
  forIn3.push(k);
}
console.log("for-in3=" + forIn3.join(","));
console.log("shadow-count=" + forIn3.filter((k) => k === "inherited").length);
console.log("shadow-value=" + m.inherited);

// A non-enumerable own property is invisible to keys/for-in but not to
// getOwnPropertyNames.
Object.defineProperty(m, "hidden", { value: 1, enumerable: false, writable: true, configurable: true });
const forIn4: string[] = [];
for (const k in m) {
  forIn4.push(k);
}
console.log("hidden-in-keys=" + (Object.keys(m).indexOf("hidden") >= 0));
console.log("hidden-in-forin=" + (forIn4.indexOf("hidden") >= 0));
console.log("hidden-in-names=" + (Object.getOwnPropertyNames(m).indexOf("hidden") >= 0));
console.log("hidden-value=" + m.hidden);

// Two levels of prototype both contribute, base last.
class SubMixed extends Mixed {
  s: string = "s";
}
(SubMixed.prototype as any).subOnly = "sub-proto";
const sm: any = new SubMixed();
const forIn5: string[] = [];
for (const k in sm) {
  forIn5.push(k);
}
console.log("sub-keys=" + Object.keys(sm).join(","));
console.log("sub-for-in=" + forIn5.join(","));
console.log("sub-tail=" + forIn5.slice(-2).join(","));
