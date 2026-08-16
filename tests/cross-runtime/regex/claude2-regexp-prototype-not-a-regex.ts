// Cross-runtime: RegExp.prototype is an ORDINARY OBJECT, not a RegExp — it has
// no [[RegExpMatcher]]. Every flag getter therefore throws on any receiver
// except one that has the slot, with a single hard-coded exemption: when the
// receiver IS %RegExp.prototype% the getters answer undefined (or "(?:)" / "")
// instead of throwing. Nothing in the corpus touches the prototype as a value.

function attempt(f: () => any): string {
  try {
    const v = f();
    return v === undefined ? "undefined" : JSON.stringify(v);
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

const P: any = RegExp.prototype;
const FLAGS = ["hasIndices", "global", "ignoreCase", "multiline", "dotAll", "unicode", "unicodeSets", "sticky"];

// --- the prototype is a plain object ---
console.log("tag=" + Object.prototype.toString.call(P));
console.log("is-regexp-instance=" + (P instanceof RegExp));
console.log("proto-of-proto=" + (Object.getPrototypeOf(P) === Object.prototype));
console.log("ctor=" + (P.constructor === RegExp));
console.log("has-lastIndex=" + Object.prototype.hasOwnProperty.call(P, "lastIndex"));

// --- the exemption: on %RegExp.prototype% itself the getters do not throw ---
console.log("proto-source=" + P.source);
console.log("proto-flags=" + JSON.stringify(P.flags));
for (let i = 0; i < FLAGS.length; i++) {
  console.log("proto-" + FLAGS[i] + "=" + String(P[FLAGS[i]]));
}
console.log("proto-toString=" + P.toString());

// --- but the methods DO throw, because they need the matcher slot ---
console.log("proto-exec=" + attempt(() => P.exec("a")));
console.log("proto-test=" + attempt(() => P.test("a")));
console.log("proto-match=" + attempt(() => "a".match(P)));
console.log("proto-search=" + attempt(() => "a".search(P)));
console.log("proto-replace=" + attempt(() => "a".replace(P, "-")));
console.log("proto-split=" + attempt(() => "a".split(P)));
console.log("proto-matchAll=" + attempt(() => [..."a".matchAll(P)].length));

// --- a plain object with the right own properties is still not a regex ---
const fake: any = { source: "a", flags: "g", global: true, lastIndex: 0 };
for (let i = 0; i < FLAGS.length; i++) {
  const g: any = Object.getOwnPropertyDescriptor(RegExp.prototype, FLAGS[i]);
  console.log("fake-" + FLAGS[i] + "=" + attempt(() => g.get.call(fake)));
}
const gs: any = Object.getOwnPropertyDescriptor(RegExp.prototype, "source");
const gf: any = Object.getOwnPropertyDescriptor(RegExp.prototype, "flags");
console.log("fake-source=" + attempt(() => gs.get.call(fake)));
console.log("fake-flags=" + attempt(() => gf.get.call(fake)));
console.log("fake-exec=" + attempt(() => RegExp.prototype.exec.call(fake, "a")));

// --- `flags` is the ONE getter that is generic: it reads the others as properties ---
console.log("flags-on-fake=" + attempt(() => gf.get.call({ global: true, sticky: true, dotAll: true })));
console.log("flags-order=" + attempt(() => gf.get.call({
  sticky: true, unicode: true, multiline: true, ignoreCase: true, global: true, hasIndices: true, dotAll: true,
})));
console.log("flags-truthy=" + attempt(() => gf.get.call({ global: 1, ignoreCase: "x" })));
console.log("flags-primitive=" + attempt(() => gf.get.call(1)));
console.log("flags-null=" + attempt(() => gf.get.call(null)));

// --- every getter lives on the prototype as an accessor, never on the instance ---
const inst = /a/gi;
for (let i = 0; i < FLAGS.length; i++) {
  console.log("own-" + FLAGS[i] + "=" + Object.prototype.hasOwnProperty.call(inst, FLAGS[i]));
}
const d: any = Object.getOwnPropertyDescriptor(RegExp.prototype, "global");
console.log("global-desc=" + (typeof d.get) + "/" + (d.set === undefined) + "/" + d.enumerable + "/" + d.configurable);

// --- toString is generic: it just concatenates source and flags ---
console.log("tostring-fake=" + RegExp.prototype.toString.call({ source: "x", flags: "y" }));
console.log("tostring-coerce=" + RegExp.prototype.toString.call({ source: 1, flags: 2 }));
console.log("tostring-missing=" + RegExp.prototype.toString.call({}));

// --- Symbol.match on the prototype makes isRegExp-style checks answer true ---
console.log("symbol-match-proto=" + (P[Symbol.match] === RegExp.prototype[Symbol.match]));
console.log("startsWith-proto=" + attempt(() => "a".startsWith(P as any)));
console.log("startsWith-flagged=" + attempt(() => "a".startsWith({ [Symbol.match]: true } as any)));
console.log("startsWith-unflagged=" + attempt(() => "a".startsWith({ [Symbol.match]: false, toString: () => "a" } as any)));
