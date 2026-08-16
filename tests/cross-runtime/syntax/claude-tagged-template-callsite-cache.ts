// Cross-runtime: the template object handed to a tag is CACHED per call site
// and frozen. Two evaluations of the same tagged template get the identical
// array; a different call site gets a different one.

function tag(strings: TemplateStringsArray, ...values: any[]): any {
  void values;
  return strings;
}

// Same call site, evaluated three times from a loop.
const fromLoop: any[] = [];
for (let i = 0; i < 3; i++) fromLoop.push(tag`a${i}b`);
console.log("loop_same_0_1=" + (fromLoop[0] === fromLoop[1]));
console.log("loop_same_1_2=" + (fromLoop[1] === fromLoop[2]));

// A second, textually identical call site is a DIFFERENT site.
const siteA = tag`a${1}b`;
const siteB = tag`a${1}b`;
console.log("distinct_sites=" + (siteA === siteB));
console.log("site_vs_loop=" + (siteA === fromLoop[0]));

// The same site inside a function, called twice.
function callSite(n: number): any {
  return tag`x${n}y`;
}
const c1 = callSite(1);
const c2 = callSite(2);
console.log("fn_site_same=" + (c1 === c2));
console.log("fn_site_vs_other=" + (c1 === siteA));

// Recursion reaches the same site, so the same object.
function recurse(n: number, acc: any[]): any[] {
  acc.push(tag`r${n}`);
  return n > 0 ? recurse(n - 1, acc) : acc;
}
const rec = recurse(3, []);
console.log("recursion_same=" + (rec[0] === rec[1] && rec[1] === rec[2] && rec[2] === rec[3]));

// Shape of the object.
const s = tag`p${1}q${2}r`;
console.log("is_array=" + Array.isArray(s));
console.log("length=" + s.length);
console.log("cooked=" + s.join("/"));
console.log("raw_is_array=" + Array.isArray(s.raw));
console.log("raw_length=" + s.raw.length);
console.log("raw_not_self=" + (s.raw === s));

// Frozen: both the cooked array and the raw array.
console.log("frozen=" + Object.isFrozen(s));
console.log("raw_frozen=" + Object.isFrozen(s.raw));
console.log("sealed=" + Object.isSealed(s));
console.log("extensible=" + Object.isExtensible(s));

// Frozen means a write is REFUSED. `Reflect.set` answers with a boolean in
// both strict and sloppy code, where a bare assignment only throws in strict.
console.log("set_index_refused=" + Reflect.set(s, 0, "changed"));
console.log("set_raw_refused=" + Reflect.set(s, "raw", []));
console.log("set_new_key_refused=" + Reflect.set(s, "extra", 1));
console.log("delete_index_refused=" + Reflect.deleteProperty(s, 0));
console.log("define_index_refused=" + Reflect.defineProperty(s, 0, { value: "changed" }));
console.log("unchanged=" + s[0]);
console.log("raw_unchanged=" + s.raw[0]);
console.log("no_extra_key=" + ("extra" in s));

// The same answer read straight off the descriptors.
const zeroDesc = Object.getOwnPropertyDescriptor(s, 0) as any;
console.log("index_writable=" + zeroDesc.writable);
console.log("index_configurable=" + zeroDesc.configurable);
console.log("not_extensible=" + !Reflect.isExtensible(s));

// The `raw` property is an own, non-enumerable data property.
const d = Object.getOwnPropertyDescriptor(s, "raw") as any;
console.log("raw_own=" + Object.prototype.hasOwnProperty.call(s, "raw"));
console.log("raw_enumerable=" + d.enumerable);
console.log("raw_writable=" + d.writable);
console.log("raw_configurable=" + d.configurable);
console.log("own_keys=" + Object.getOwnPropertyNames(s).join(","));

// Values are NOT cached — only the strings object is.
const collected: string[] = [];
function collect(strings: TemplateStringsArray, ...values: any[]): string {
  collected.push(values.join("+"));
  return strings.raw.join("_");
}
for (let i = 0; i < 3; i++) collect`v${i}w${i * 2}`;
console.log("values=" + collected.join("|"));

// The cache survives the tag being a different function at the same site.
function tagOne(strings: TemplateStringsArray): any { return strings; }
function tagTwo(strings: TemplateStringsArray): any { return strings; }
const which: any[] = [];
for (let i = 0; i < 2; i++) {
  const fn = i === 0 ? tagOne : tagTwo;
  which.push(fn`shared${i}`);
}
console.log("cache_by_site_not_tag=" + (which[0] === which[1]));

// String.raw over the same site.
const rawOut: string[] = [];
for (let i = 0; i < 2; i++) rawOut.push(String.raw`n\t${i}`);
console.log("string_raw=" + rawOut.join("|"));
