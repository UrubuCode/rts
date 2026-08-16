// Cross-runtime: how a Set operation VALIDATES its argument (GetSetRecord).
// The three properties are read in a fixed order — size, then has, then keys —
// a bad `size` is a TypeError or a RangeError depending on WHY it is bad, and
// the receiver's brand is checked before the argument is touched at all.

const base = new Set([1, 2, 3]);

function show(v: any): string {
  if (v instanceof Set) return "{" + [...v].join(",") + "}";
  return String(v);
}

function probe(label: string, fn: () => any): void {
  try {
    console.log(label + "=ok:" + show(fn()));
  } catch (e: any) {
    console.log(label + "=" + e.constructor.name);
  }
}

// A set-like whose three properties are accessors, so the READ order is visible.
function record(size: any, has: any, keys: any, seen: string[]): any {
  return {
    get size() { seen.push("size"); return size; },
    get has() { seen.push("has"); return has; },
    get keys() { seen.push("keys"); return keys; },
  };
}

const goodHas = function (v: any) { return v === 2 || v === 9; };
const goodKeys = function () { let i = 0; const vals = [2, 9]; return { next() { return i < vals.length ? { value: vals[i++], done: false } : { value: undefined, done: true }; } }; };

// --- the happy path reads all three, once each, in order ---
const seenOk: string[] = [];
console.log("valid_union=" + show((base as any).union(record(2, goodHas, goodKeys, seenOk))));
console.log("read_order=" + seenOk.join(","));

// --- a bad `has` stops after size+has; a bad `keys` stops after all three ---
const seenHas: string[] = [];
probe("has_not_callable", () => (base as any).union(record(2, 42, goodKeys, seenHas)));
console.log("has_stop=" + seenHas.join(","));

const seenKeys: string[] = [];
probe("keys_not_callable", () => (base as any).union(record(2, goodHas, "nope", seenKeys)));
console.log("keys_stop=" + seenKeys.join(","));

const seenSize: string[] = [];
probe("size_bad_stops_early", () => (base as any).union(record(undefined, goodHas, goodKeys, seenSize)));
console.log("size_stop=" + seenSize.join(","));

// --- the receiver's brand is checked BEFORE the argument is read ---
const seenBrand: string[] = [];
probe("wrong_receiver", () => (Set.prototype as any).union.call([1, 2], record(2, goodHas, goodKeys, seenBrand)));
console.log("brand_before_arg=" + seenBrand.length);
probe("receiver_is_map", () => (Set.prototype as any).union.call(new Map(), new Set([1])));
probe("receiver_is_prototype", () => (Set.prototype as any).union(new Set([1])));

// --- what makes a `size` bad: NaN is a TypeError, negative is a RangeError ---
function withSize(s: any): any {
  return { size: s, has: goodHas, keys: goodKeys };
}
probe("size_undefined", () => (base as any).union(withSize(undefined)));
probe("size_nan", () => (base as any).union(withSize(NaN)));
probe("size_negative", () => (base as any).union(withSize(-1)));
probe("size_neg_infinity", () => (base as any).union(withSize(-Infinity)));
probe("size_negative_zero", () => (base as any).union(withSize(-0)));
probe("size_infinity", () => (base as any).union(withSize(Infinity)));
probe("size_bigint", () => (base as any).union(withSize(10n)));
probe("size_symbol", () => (base as any).union(withSize(Symbol("s"))));
probe("size_string_numeric", () => (base as any).union(withSize("2")));
probe("size_string_junk", () => (base as any).union(withSize("two")));
probe("size_null", () => (base as any).union(withSize(null)));
probe("size_true", () => (base as any).union(withSize(true)));
probe("size_fractional", () => (base as any).union(withSize(1.9)));
probe("size_valueOf", () => (base as any).union(withSize({ valueOf() { return 2; } })));
probe("size_getter_throws", () => (base as any).union({ get size(): any { throw new EvalError("size"); }, has: goodHas, keys: goodKeys }));

// --- a non-object argument never gets that far ---
probe("arg_number", () => (base as any).union(3));
probe("arg_string", () => (base as any).union("ab"));
probe("arg_null", () => (base as any).union(null));
probe("arg_undefined", () => (base as any).union(undefined));
probe("arg_missing", () => (base as any).union());
probe("arg_array", () => (base as any).union([1, 2]));
probe("arg_symbol", () => (base as any).union(Symbol("x")));

// --- every one of the seven ops validates the same way ---
const opNames = ["union", "intersection", "difference", "symmetricDifference", "isSubsetOf", "isSupersetOf", "isDisjointFrom"];
let sameForAll = "";
for (const op of opNames) {
  let tag: string;
  try { (base as any)[op](withSize(-1)); tag = "no_throw"; }
  catch (e: any) { tag = e.constructor.name; }
  sameForAll += op + ":" + tag + ";";
}
console.log("negative_size_all_ops=" + sameForAll);

let sameForAll2 = "";
for (const op of opNames) {
  let tag: string;
  try { (base as any)[op]({ size: 1, has: 1, keys: goodKeys }); tag = "no_throw"; }
  catch (e: any) { tag = e.constructor.name; }
  sameForAll2 += op + ":" + tag + ";";
}
console.log("bad_has_all_ops=" + sameForAll2);

// --- the declared size is TRUSTED, and a lie changes the answer ---
const liar = { size: 0, has: function (v: any) { return v === 2; }, keys: goodKeys };
console.log("lie_small_intersection=" + show((base as any).intersection(liar)));
console.log("lie_small_isDisjointFrom=" + (base as any).isDisjointFrom(liar));
const liarBig = { size: 1000, has: function (v: any) { return v === 2; }, keys: goodKeys };
console.log("lie_big_intersection=" + show((base as any).intersection(liarBig)));
console.log("lie_big_isDisjointFrom=" + (base as any).isDisjointFrom(liarBig));
console.log("lie_big_isSubsetOf=" + (base as any).isSubsetOf(liarBig));
console.log("lie_small_isSupersetOf=" + (base as any).isSupersetOf(liar));
