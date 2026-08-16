// Cross-runtime: WHICH built-in operations ask Symbol.species for the
// constructor of their result. The accessor is uniform across the constructors
// that carry it, the derived-copy methods consult it, and the newer
// change-by-copy methods deliberately do not.

// --- the accessor itself is the same shape everywhere it exists ---
function speciesShape(label: string, C: any): void {
  const d: any = Object.getOwnPropertyDescriptor(C, Symbol.species);
  if (d === undefined) { console.log(label + "=absent"); return; }
  console.log(label + "=" + typeof d.get + ":" + d.get.name + ":len=" + d.get.length +
    ":set=" + typeof d.set + ":" + d.enumerable + ":" + d.configurable +
    ":returns_this=" + (d.get.call(C) === C));
}
speciesShape("array", Array);
speciesShape("map", Map);
speciesShape("set", Set);
speciesShape("promise", Promise);
speciesShape("regexp", RegExp);
speciesShape("arraybuffer", ArrayBuffer);
speciesShape("object", Object);
speciesShape("string", String);
speciesShape("weakmap", WeakMap);

// --- an Array subclass: which methods hand back the subclass ---
class Sub extends Array<number> {}
const sub = Sub.from([3, 1, 2]) as any;
console.log("from_is_subclass=" + (sub instanceof Sub));

function kind(label: string, v: any): void {
  console.log(label + "=" + (v instanceof Sub ? "Sub" : Array.isArray(v) ? "Array" : typeof v));
}
kind("map", sub.map((x: number) => x));
kind("filter", sub.filter(() => true));
kind("slice", sub.slice(0));
kind("splice", sub.slice().splice(0, 1));
kind("concat", sub.concat([9]));
kind("flat", sub.flat());
kind("flatMap", sub.flatMap((x: number) => [x]));
kind("toReversed", sub.toReversed());
kind("toSorted", sub.toSorted());
kind("toSpliced", sub.toSpliced(0, 1));
kind("with", sub.with(0, 5));
kind("reverse_in_place", sub.reverse());
kind("sort_in_place", sub.sort());
kind("array_of", Sub.of(1, 2));

// --- pointing species at Array gives plain arrays back ---
class Plain extends Array<number> {
  static get [Symbol.species]() { return Array; }
}
const plain = Plain.from([1, 2]) as any;
console.log("plain_self=" + (plain instanceof Plain));
console.log("plain_map=" + (plain.map((x: number) => x) instanceof Plain) + ":" + Array.isArray(plain.map((x: number) => x)));
console.log("plain_filter_proto=" + (Object.getPrototypeOf(plain.filter(() => true)) === Array.prototype));

// --- null and undefined mean "use the default"; a non-constructor throws ---
function speciesResult(value: any): string {
  class Custom extends Array<number> {
    static get [Symbol.species]() { return value; }
  }
  try {
    const r = (Custom.from([1, 2]) as any).map((x: number) => x);
    return "ok:" + (r instanceof Custom ? "Custom" : Array.isArray(r) ? "Array" : typeof r);
  } catch (e: any) {
    return e.constructor.name;
  }
}
console.log("species_undefined=" + speciesResult(undefined));
console.log("species_null=" + speciesResult(null));
console.log("species_number=" + speciesResult(42));
console.log("species_plain_object=" + speciesResult({}));
console.log("species_arrow=" + speciesResult(() => 1));
console.log("species_map_ctor=" + speciesResult(Map));

// --- a species constructor is CALLED with the length ---
const seenArgs: string[] = [];
class Recording extends Array<number> {
  static get [Symbol.species]() {
    return function (this: any, ...args: any[]) {
      seenArgs.push(args.length + ":" + args.map(String).join("/"));
      return new Array(...args);
    } as any;
  }
}
const rec = Recording.from([1, 2, 3]) as any;
rec.filter((x: number) => x > 1);
rec.slice(1);
console.log("species_call_args=" + seenArgs.join("|"));

// --- Map and Set carry the accessor but nothing consults it ---
class MapSub extends Map<string, number> {
  static get [Symbol.species]() { return Map; }
}
const ms = new MapSub([["a", 1]]);
console.log("mapsub_self=" + (ms instanceof MapSub));
console.log("mapsub_entries_iter=" + (ms.entries().next().value as any).join(":"));
class SetSub extends Set<number> {
  static get [Symbol.species]() { return Set; }
}
const ss = new SetSub([1, 2]);
console.log("setsub_union_kind=" + (ss.union(new Set([3])) instanceof SetSub) + ":" + (ss.union(new Set([3])) instanceof Set));

// --- Promise#then DOES consult it, and that is observable synchronously ---
class PromiseSub extends Promise<number> {}
const ps = PromiseSub.resolve(1);
console.log("promise_resolve_kind=" + (ps instanceof PromiseSub));
console.log("promise_then_kind=" + (ps.then(() => 1) instanceof PromiseSub));
console.log("promise_catch_kind=" + (ps.catch(() => 1) instanceof PromiseSub));
console.log("promise_finally_kind=" + (ps.finally(() => 1) instanceof PromiseSub));

class PlainPromise extends Promise<number> {
  static get [Symbol.species]() { return Promise; }
}
const pp = PlainPromise.resolve(1);
console.log("promise_species_plain=" + (pp instanceof PlainPromise) + ":" + (pp.then(() => 1) instanceof PlainPromise));

// --- ArrayBuffer#slice consults it ---
class BufSub extends ArrayBuffer {}
const buf = new BufSub(8);
console.log("buffer_slice_kind=" + (buf.slice(0, 4) instanceof BufSub) + ":" + (buf.slice(0, 4) instanceof ArrayBuffer));
console.log("buffer_slice_len=" + buf.slice(0, 4).byteLength);

class PlainBuf extends ArrayBuffer {
  static get [Symbol.species]() { return ArrayBuffer; }
}
console.log("buffer_species_plain=" + (new PlainBuf(8).slice(0, 4) instanceof PlainBuf));

// --- RegExp[Symbol.split] consults it, so a subclass drives the split ---
let splitCtorCalls = 0;
class RegSub extends RegExp {
  static get [Symbol.species]() {
    splitCtorCalls++;
    return RegExp;
  }
}
console.log("regexp_split=" + "a1b2c".split(new RegSub("\\d", "") as any).join(","));
console.log("regexp_species_consulted=" + (splitCtorCalls > 0));

// --- a typed array's copying methods consult it too ---
class U8Sub extends Uint8Array {}
const u8 = new U8Sub([1, 2, 3]);
console.log("typedarray_subarray=" + (u8.subarray(1) instanceof U8Sub));
console.log("typedarray_slice=" + (u8.slice(1) instanceof U8Sub));
console.log("typedarray_map=" + (u8.map((x: number) => x) instanceof U8Sub));
console.log("typedarray_filter=" + (u8.filter(() => true) instanceof U8Sub));
