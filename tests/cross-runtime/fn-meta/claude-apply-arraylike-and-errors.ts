// Cross-runtime: what `Function.prototype.apply` accepts as its second argument
// — an array-like read through `length`, `null`/`undefined` meaning no
// arguments, a non-object being a TypeError, and an absurd `length` a RangeError.

function count(...args: any[]): string {
  return args.length + ":[" + args.map((a) => String(a)).join(",") + "]";
}

// A real array.
console.log("array=" + count.apply(null, [1, 2, 3]));

// A plain array-like: only indices below `length` are read.
console.log("arraylike=" + count.apply(null, { 0: "a", 1: "b", length: 2 } as any));
console.log("extra_ignored=" + count.apply(null, { 0: "a", 1: "b", 2: "c", length: 2 } as any));
console.log("holes_become_undefined=" + count.apply(null, { 0: "a", length: 3 } as any));

// `length` is coerced to an integer.
console.log("length_string=" + count.apply(null, { 0: "a", 1: "b", length: "2" } as any));
console.log("length_float=" + count.apply(null, { 0: "a", 1: "b", length: 2.9 } as any));
console.log("length_negative=" + count.apply(null, { 0: "a", length: -1 } as any));
console.log("length_missing=" + count.apply(null, { 0: "a" } as any));
console.log("length_nan=" + count.apply(null, { 0: "a", length: NaN } as any));
console.log("length_bool=" + count.apply(null, { 0: "a", 1: "b", length: true } as any));

// `arguments` is an array-like.
function forward(): string {
  return count.apply(null, arguments as any);
}
console.log("arguments=" + forward(1, 2 as any, 3 as any));

// A Set is NOT array-like (no `length`), so it contributes nothing.
console.log("set=" + count.apply(null, new Set([1, 2, 3]) as any));

// null / undefined mean "no arguments at all".
console.log("null_args=" + count.apply(null, null));
console.log("undefined_args=" + count.apply(null, undefined));
console.log("omitted_args=" + count.apply(null));

// A primitive that is not null/undefined is a TypeError.
function applyErr(argsList: any): string {
  try {
    (count as any).apply(null, argsList);
    return "ok";
  } catch (e) {
    return (e as any).constructor.name;
  }
}
console.log("number_args=" + applyErr(1));
console.log("boolean_args=" + applyErr(true));
console.log("string_args=" + applyErr("abc"));
console.log("symbol_args=" + applyErr(Symbol("s")));
console.log("bigint_args=" + applyErr(10n));

// An absurd `length` is a RangeError, not an out-of-memory.
console.log("huge_length=" + applyErr({ length: 2 ** 53 - 1 }));
console.log("infinite_length=" + applyErr({ length: Infinity }));

// A getter on `length` runs exactly once.
let lengthReads = 0;
const watched: any = {
  0: "x",
  1: "y",
  get length(): number { lengthReads += 1; return 2; },
};
console.log("watched=" + count.apply(null, watched));
console.log("length_reads=" + lengthReads);

// Index getters run in order.
const order: string[] = [];
const ordered: any = {
  get 0() { order.push("i0"); return "a"; },
  get 1() { order.push("i1"); return "b"; },
  get 2() { order.push("i2"); return "c"; },
  length: 3,
};
console.log("ordered=" + count.apply(null, ordered));
console.log("index_order=" + order.join(","));

// Inherited indices are visible through the prototype chain.
const base: any = { 1: "inherited" };
const derived: any = Object.create(base);
derived[0] = "own";
derived.length = 2;
console.log("inherited_index=" + count.apply(null, derived));

// `call` compared with `apply` on the same receiver.
function receiver(this: any, a: any, b: any): string {
  return String(this === undefined ? "undefined-this" : this.tag) + "|" + a + "|" + b;
}
console.log("call=" + receiver.call({ tag: "R" }, 1, 2));
console.log("apply=" + receiver.apply({ tag: "R" }, [1, 2]));
console.log("apply_short=" + receiver.apply({ tag: "R" }, [1] as any));
console.log("apply_long=" + receiver.apply({ tag: "R" }, [1, 2, 3] as any));

// `apply` on a bound function still prepends the bound arguments.
const boundCount = count.bind(null, "bound");
console.log("bound_apply=" + boundCount.apply(null, [1, 2] as any));

// `Reflect.apply` needs a real array-like too and rejects null.
try {
  (Reflect as any).apply(count, null, null);
  console.log("reflect_null=ok");
} catch (e) {
  console.log("reflect_null_threw=" + (e as any).constructor.name);
}
console.log("reflect_arraylike=" + Reflect.apply(count, null, { 0: 1, length: 1 } as any));
