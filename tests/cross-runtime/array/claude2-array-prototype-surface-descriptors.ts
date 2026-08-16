// ONE thing: the SHAPE of Array's own surface — every prototype method is a
// non-enumerable, writable, configurable function with a specified name and
// length, and Array.prototype itself is an array. An engine that installs
// methods as plain enumerable properties passes every behaviour test and fails
// this one.
const protoMethods = [
  "at", "concat", "copyWithin", "entries", "every", "fill", "filter", "find",
  "findIndex", "findLast", "findLastIndex", "flat", "flatMap", "forEach",
  "includes", "indexOf", "join", "keys", "lastIndexOf", "map", "pop", "push",
  "reduce", "reduceRight", "reverse", "shift", "slice", "some", "sort",
  "splice", "toLocaleString", "toReversed", "toSorted", "toSpliced", "toString",
  "unshift", "values", "with",
];

const missing: string[] = [];
const badShape: string[] = [];
for (const m of protoMethods) {
  const d: any = Object.getOwnPropertyDescriptor(Array.prototype, m);
  if (!d) { missing.push(m); continue; }
  if (!(typeof d.value === "function" && d.writable === true && d.enumerable === false && d.configurable === true)) {
    badShape.push(m + "{w:" + d.writable + ",e:" + d.enumerable + ",c:" + d.configurable + ",t:" + typeof d.value + "}");
  }
}
console.log("missing=" + (missing.length ? missing.join(",") : "none"));
console.log("badShape=" + (badShape.length ? badShape.join(" ") : "none"));

// name and length are part of the contract.
console.log("lengths=" + protoMethods.map((m) => m + ":" + (Array.prototype as any)[m].length).join(" "));
console.log("names=" + protoMethods.filter((m) => (Array.prototype as any)[m].name !== m).join(",") + "|");

// Array.prototype is itself an array with length 0.
console.log("protoIsArray=" + Array.isArray(Array.prototype) + " protoLen=" + Array.prototype.length);
console.log("protoOf=" + (Object.getPrototypeOf(Array.prototype) === Object.prototype));
console.log("protoTag=" + Object.prototype.toString.call(Array.prototype));

// The statics.
for (const s of ["from", "of", "isArray", "fromAsync"]) {
  const d: any = Object.getOwnPropertyDescriptor(Array, s);
  console.log("static." + s + "=" + (d ? "w:" + d.writable + ",e:" + d.enumerable + ",c:" + d.configurable + ",len:" + (d.value ? d.value.length : "?") : "absent"));
}

// Array itself.
const lenDesc: any = Object.getOwnPropertyDescriptor(Array, "length");
console.log("Array.length=" + Array.length + " desc=" + lenDesc.writable + "," + lenDesc.enumerable + "," + lenDesc.configurable);
console.log("Array.name=" + Array.name);
console.log("ctorOnProto=" + (Array.prototype.constructor === Array));
const ctorDesc: any = Object.getOwnPropertyDescriptor(Array.prototype, "constructor");
console.log("ctorDesc=" + ctorDesc.writable + "," + ctorDesc.enumerable + "," + ctorDesc.configurable);
console.log("speciesIsGetter=" + (typeof Object.getOwnPropertyDescriptor(Array, Symbol.species)!.get));

// Symbol-keyed own properties of Array.prototype: iterator and unscopables.
const syms = Object.getOwnPropertySymbols(Array.prototype).map((s) => String(s)).sort();
console.log("protoSymbols=" + syms.join(","));
console.log("iteratorIsValues=" + ((Array.prototype as any)[Symbol.iterator] === Array.prototype.values));
const unsc: any = (Array.prototype as any)[Symbol.unscopables];
console.log("unscopablesProto=" + String(Object.getPrototypeOf(unsc)));
console.log("unscopablesKeys=" + Object.keys(unsc).sort().join(","));
console.log("unscopablesValues=" + Object.keys(unsc).every((k) => unsc[k] === true));

// None of the prototype methods is enumerable, so for-in over an array finds
// only its own index keys.
const seen: string[] = [];
for (const k in [1, 2]) seen.push(k);
console.log("forIn=" + seen.join(",") + " keysOfProto=" + Object.keys(Array.prototype).length);
