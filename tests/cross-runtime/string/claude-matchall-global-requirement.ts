// Cross-runtime: matchAll REFUSES a non-global regex with a TypeError, and works
// on an internal CLONE — the caller's regex keeps its lastIndex untouched while
// the iterator advances. 261_string_matchall.ts only spreads a global regex once.

// --- the global flag is mandatory ---
try {
  console.log("nog=" + [..."aa".matchAll(/a/ as any)].length);
} catch (e: any) {
  console.log("nog!" + e.constructor.name);
}
try {
  console.log("sticky-only=" + [..."aa".matchAll(/a/y as any)].length);
} catch (e: any) {
  console.log("sticky-only!" + e.constructor.name);
}
console.log("g=" + [..."aa".matchAll(/a/g)].length);
console.log("gy=" + [..."aab".matchAll(/a/gy)].length);
console.log("gi=" + [..."aA".matchAll(/a/gi)].length);

// --- a STRING argument is turned into a global regex, so it never throws ---
console.log("str-arg=" + [..."a.a".matchAll("a" as any)].length);
console.log("str-arg-meta=" + [..."a.b".matchAll("." as any)].length);
console.log("no-arg=" + [..."ab".matchAll(undefined as any)].length);

// --- the caller's lastIndex is READ once but never written ---
const re = /a/g;
re.lastIndex = 1;
const it = "aaa".matchAll(re);
console.log("clone-count=" + [...it].length);
console.log("clone-lastIndex=" + re.lastIndex);

const re2 = /a/g;
re2.lastIndex = 0;
console.log("from-zero=" + [..."aaa".matchAll(re2)].length + ":" + re2.lastIndex);

// --- the iterator is lazy: pulling one entry does not run the rest ---
const lazy = "abcabc".matchAll(/[abc]/g);
const first: any = lazy.next();
console.log("lazy1=" + first.value[0] + ":" + first.value.index + ":" + first.done);
const second: any = lazy.next();
console.log("lazy2=" + second.value[0] + ":" + second.value.index);
console.log("lazy-rest=" + [...lazy].map((m: any) => m[0] + m.index).join(","));
const done: any = lazy.next();
console.log("lazy-exhausted=" + String(done.value) + ":" + done.done);
console.log("lazy-again=" + [...lazy].length);

// --- each entry is a full exec result ---
const entries = [..."a1b2".matchAll(/(?<l>[a-z])(\d)/g)];
console.log("entry-count=" + entries.length);
const e0: any = entries[0];
console.log("entry0=" + e0[0] + "|" + e0[1] + "|" + e0[2] + "|" + e0.index + "|" + e0.input);
console.log("entry0-groups=" + JSON.stringify(e0.groups));
console.log("entry0-isarray=" + Array.isArray(e0));
console.log("entry0-len=" + e0.length);
const e1: any = entries[1];
console.log("entry1-index=" + e1.index + ":" + e1.groups.l);

// --- zero-length matches advance by one and terminate ---
console.log("zero-len=" + [..."ab".matchAll(/(?:)/g)].map((m: any) => m.index).join(","));
console.log("zero-len-u=" + [..."\u{1F600}b".matchAll(/(?:)/gu)].map((m: any) => m.index).join(","));
console.log("zero-len-nou=" + [..."\u{1F600}b".matchAll(/(?:)/g)].map((m: any) => m.index).join(","));
console.log("astral-star=" + [..."\u{1F600}".matchAll(/x*/gu)].length);

// --- no match yields an empty iterator, not null (unlike match) ---
console.log("nomatch=" + [..."abc".matchAll(/z/g)].length);
console.log("match-nomatch=" + String("abc".match(/z/g)));

// --- the iterator is its own iterable ---
const self = "ab".matchAll(/a/g);
console.log("self-iterable=" + ((self as any)[Symbol.iterator]() === self));
console.log("tag=" + Object.prototype.toString.call("ab".matchAll(/a/g)));

// --- matchAll never mutates the subject-derived state between calls ---
const shared = /[ab]/g;
console.log("run1=" + [..."ab".matchAll(shared)].length);
console.log("run2=" + [..."ab".matchAll(shared)].length);
console.log("shared-lastIndex=" + shared.lastIndex);
