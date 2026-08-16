// Cross-runtime: WHICH side of a Set operation is walked. Each op either drains
// the argument's keys() or probes it with has(), never both arbitrarily — and
// the receiver's own methods are never consulted, because the ops read the
// internal data directly.

function tracker(values: number[], sizeLie?: number): any {
  const calls = { size: 0, has: 0, keys: 0, next: 0, ret: 0 };
  const obj: any = {
    get size() { calls.size++; return sizeLie === undefined ? values.length : sizeLie; },
    has(v: any) { calls.has++; return values.indexOf(v) >= 0; },
    keys() {
      calls.keys++;
      let i = 0;
      return {
        next() {
          calls.next++;
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return(v: any) { calls.ret++; return { done: true, value: v }; },
      };
    },
  };
  return { obj, calls };
}

function tally(c: any): string {
  return "size=" + c.size + ",has=" + c.has + ",keys=" + c.keys + ",next=" + c.next + ",return=" + c.ret;
}

const receiver = new Set([1, 2, 3]);
const argValues = [2, 3, 4];

// --- the per-operation call profile, receiver and argument the same size ---
const ops = ["union", "intersection", "difference", "symmetricDifference", "isSubsetOf", "isSupersetOf", "isDisjointFrom"];
for (const op of ops) {
  const t = tracker(argValues);
  const r = (receiver as any)[op](t.obj);
  const out = r instanceof Set ? "{" + [...r].join(",") + "}" : String(r);
  console.log(op + "=" + out + " " + tally(t.calls));
}

// --- a SMALLER argument flips which side intersection/difference walk ---
for (const op of ["intersection", "difference", "isDisjointFrom"]) {
  const t = tracker([2], 1);
  const r = (receiver as any)[op](t.obj);
  const out = r instanceof Set ? "{" + [...r].join(",") + "}" : String(r);
  console.log("small_" + op + "=" + out + " " + tally(t.calls));
}

// --- a LARGER argument flips them back ---
for (const op of ["intersection", "difference", "isDisjointFrom"]) {
  const t = tracker([2, 3, 4, 5, 6, 7], 6);
  const r = (receiver as any)[op](t.obj);
  const out = r instanceof Set ? "{" + [...r].join(",") + "}" : String(r);
  console.log("large_" + op + "=" + out + " " + tally(t.calls));
}

// --- an empty receiver still validates and may still walk the argument ---
const empty = new Set<number>();
for (const op of ops) {
  const t = tracker(argValues);
  (empty as any)[op](t.obj);
  console.log("empty_" + op + "=" + tally(t.calls));
}

// --- an early exit CLOSES the argument's key iterator ---
const early = tracker([9, 1, 2], 3);
console.log("superset_false=" + (receiver as any).isSupersetOf(early.obj));
console.log("superset_closed=" + tally(early.calls));

const earlySym = tracker([1, 2, 3, 4], 4);
console.log("symdiff_ok=" + (receiver as any).symmetricDifference(earlySym.obj).size);
console.log("symdiff_not_closed=" + tally(earlySym.calls));

// --- the receiver's OWN patched methods are ignored ---
const patched: any = new Set([1, 2]);
let patchedHits = 0;
patched.has = function () { patchedHits++; return true; };
patched.add = function () { patchedHits++; return this; };
patched.keys = function () { patchedHits++; return [][Symbol.iterator](); };
console.log("patched_union=" + [...patched.union(new Set([3]))].join(","));
console.log("patched_subset=" + patched.isSubsetOf(new Set([1, 2, 3])));
console.log("patched_inter=" + [...patched.intersection(new Set([2]))].join(","));
console.log("patched_hits=" + patchedHits);

// --- a subclass overriding add/has does not shape the result either ---
class Loud extends Set<number> {
  static hits = 0;
  add(v: number): this { Loud.hits++; return super.add(v); }
  has(v: number): boolean { Loud.hits++; return super.has(v); }
}
const loud = new Loud([1, 2]);
Loud.hits = 0;
const loudUnion = loud.union(new Set([3]));
console.log("subclass_union=" + [...loudUnion].join(","));
console.log("subclass_hits=" + Loud.hits);
console.log("result_is_plain_set=" + (loudUnion instanceof Loud) + ":" + (loudUnion instanceof Set));
console.log("result_proto=" + (Object.getPrototypeOf(loudUnion) === Set.prototype));

// --- a real Set argument reaches the same answers as the set-like above ---
// (whether the ops observe `Set.prototype.size` for a genuine Set is NOT pinned:
// one engine takes a fast path that skips the accessor, and the observable
// result is identical either way.)
const plainArg = new Set([2, 3, 4]);
console.log("real_set_intersection=" + [...receiver.intersection(plainArg)].join(","));
console.log("real_set_difference=" + [...receiver.difference(plainArg)].join(","));
console.log("real_set_union=" + [...receiver.union(plainArg)].join(","));
console.log("matches_setlike=" + (
  [...receiver.intersection(plainArg)].join(",") ===
  [...(receiver as any).intersection(tracker(argValues).obj)].join(",")
));
