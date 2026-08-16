// Cross-runtime: the string iterator is an OBJECT with a shape of its own —
// %StringIteratorPrototype% with a Symbol.toStringTag, a self-returning
// Symbol.iterator, and no `return` — and it snapshots the string, so it is
// unaffected by anything done to the variable afterwards. The corpus spreads
// strings; nothing pins the iterator object itself.

const S = "a\u{1F600}b";

function codes(s: string): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i++) out.push(s.charCodeAt(i).toString(16));
  return out.join(",");
}

function attempt(f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- the object ---
const it: any = S[Symbol.iterator]();
console.log("typeof=" + typeof it);
console.log("tag=" + Object.prototype.toString.call(it));
console.log("toStringTag=" + it[Symbol.toStringTag]);
console.log("self-iterable=" + (it[Symbol.iterator]() === it));
console.log("has-next=" + (typeof it.next));
console.log("has-return=" + (typeof it.return));
console.log("has-throw=" + (typeof it.throw));
console.log("own-props=" + JSON.stringify(Object.getOwnPropertyNames(it)));
console.log("next-is-own=" + Object.prototype.hasOwnProperty.call(it, "next"));

// --- its prototype chain: StringIterator -> %IteratorPrototype% -> Object ---
const proto: any = Object.getPrototypeOf(it);
const iterProto: any = Object.getPrototypeOf(proto);
console.log("proto-has-next=" + Object.prototype.hasOwnProperty.call(proto, "next"));
console.log("iterProto-has-symbol=" + Object.prototype.hasOwnProperty.call(iterProto, Symbol.iterator));
console.log("iterProto-is-shared=" + (iterProto === Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]()))));
console.log("iterProto-parent=" + (Object.getPrototypeOf(iterProto) === Object.prototype));
console.log("next-name=" + proto.next.name + "/" + proto.next.length);
const tagDesc: any = Object.getOwnPropertyDescriptor(proto, Symbol.toStringTag);
console.log("tag-desc=" + JSON.stringify(tagDesc.value) + "/" + tagDesc.writable + "/" + tagDesc.enumerable + "/" + tagDesc.configurable);

// --- stepping it by hand, over an astral character ---
const step: any = S[Symbol.iterator]();
for (let i = 0; i < 5; i++) {
  const r: any = step.next();
  console.log("step" + i + " done=" + r.done + " value=" + (r.value === undefined ? "u" : codes(r.value)) + " len=" + (r.value === undefined ? "-" : r.value.length));
}
console.log("result-keys=" + JSON.stringify(Object.keys(S[Symbol.iterator]().next())));

// --- it is a SNAPSHOT: the iterated string cannot change under it ---
let mutable = "ab";
const snap: any = mutable[Symbol.iterator]();
console.log("snap1=" + snap.next().value);
mutable = "zz";
console.log("snap2=" + snap.next().value);
console.log("snap3=" + String(snap.next().value) + "/" + mutable);

// --- next() on a plain object is a TypeError: it needs the internal slot ---
console.log("next-detached=" + attempt(() => proto.next.call({})));
console.log("next-on-string=" + attempt(() => proto.next.call("ab")));
console.log("next-on-array-iter=" + attempt(() => proto.next.call([][Symbol.iterator]())));

// --- a spent iterator keeps answering done, forever ---
const spent: any = "a"[Symbol.iterator]();
spent.next();
console.log("spent1=" + JSON.stringify(spent.next()));
console.log("spent2=" + JSON.stringify(spent.next()));

// --- for-of, spread and Array.from all drive the same iterator ---
console.log("for-of=" + (function () {
  const out: string[] = [];
  for (const c of S) out.push(codes(c));
  return out.join("|");
})());
console.log("spread=" + [...S].map(codes).join("|"));
console.log("from=" + Array.from(S).map(codes).join("|"));
console.log("from-mapfn=" + Array.from(S, (c, i) => i + ":" + c.length).join("|"));
console.log("destructure=" + (function () {
  const [x, y, z, w] = S;
  return [codes(x), codes(y), codes(z), String(w)].join("|");
})());

// --- and split("") does NOT: it is unit-based, so the counts differ ---
console.log("split-len=" + S.split("").length + " vs iterator=" + [...S].length);

// --- a LONE surrogate is yielded whole, as one iteration ---
const lone = "a" + String.fromCharCode(0xd83d) + "b";
console.log("lone-count=" + [...lone].length);
console.log("lone-codes=" + [...lone].map(codes).join("|"));
const reversed = String.fromCharCode(0xde00) + String.fromCharCode(0xd83d);
console.log("reversed-count=" + [...reversed].length);
console.log("reversed-codes=" + [...reversed].map(codes).join("|"));

// --- deleting Symbol.iterator from the prototype is possible, and observable ---
const boxed: any = new String("ab");
console.log("boxed-spread=" + [...boxed].join("|"));
console.log("boxed-iter-same=" + (boxed[Symbol.iterator] === String.prototype[Symbol.iterator]));
console.log("iterator-fn-name=" + String.prototype[Symbol.iterator].name);
console.log("iterator-fn-length=" + String.prototype[Symbol.iterator].length);

// --- calling the iterator method on a non-string receiver ---
console.log("iter-on-number=" + attempt(() => [...(String.prototype[Symbol.iterator].call(12) as any)].join("|")));
console.log("iter-on-null=" + attempt(() => String.prototype[Symbol.iterator].call(null)));

// --- an empty string yields nothing at all ---
const empty: any = ""[Symbol.iterator]();
console.log("empty=" + JSON.stringify(empty.next()));
console.log("empty-spread-len=" + [...""].length);
