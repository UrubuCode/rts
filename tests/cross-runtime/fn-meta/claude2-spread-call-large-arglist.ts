// Cross-runtime: passing many arguments — spread in a call, `apply` with an
// array, and the interleaving of fixed arguments with spreads. The count, the
// order and the evaluation order of the operands are what is pinned.

function sum(...values: number[]): number {
  let total = 0;
  for (let i = 0; i < values.length; i++) total += values[i];
  return total;
}
function countArgs(): number {
  return arguments.length;
}
function firstLast(...values: any[]): string {
  return values.length + ":" + String(values[0]) + ".." + String(values[values.length - 1]);
}

const many: number[] = [];
for (let i = 1; i <= 128; i++) many.push(i);

// 1) A spread of 128 values arrives as 128 arguments in order.
console.log("spread_sum=" + sum(...many));
console.log("spread_shape=" + firstLast(...many));
console.log("apply_sum=" + sum.apply(null, many));
console.log("apply_shape=" + firstLast.apply(null, many));
console.log("spread_equals_apply=" + (sum(...many) === sum.apply(null, many)));

// 2) `Math.max` over the same list, both ways.
console.log("max_spread=" + Math.max(...many));
console.log("max_apply=" + Math.max.apply(null, many));
console.log("min_spread=" + Math.min(...many));

// 3) Fixed arguments and spreads interleave positionally.
console.log("interleaved=" + firstLast(0, ...[1, 2], 3, ...[4, 5]));
console.log("interleaved_sum=" + sum(1, ...[2, 3], 4, ...[5]));
console.log("spread_only_middle=" + firstLast("a", ...["b"], "c"));

// 4) Two spreads of the same array are two independent expansions.
const small = [7, 8];
console.log("double_spread=" + sum(...small, ...small));

// 5) An empty spread contributes nothing, and neither does a spread of an
//    empty string or an empty Set.
console.log("empty_array=" + countArgs(...[]));
console.log("empty_string=" + countArgs(..."" as any));
console.log("empty_set=" + countArgs(...new Set()));
console.log("empty_between=" + firstLast(1, ...[], 2));

// 6) Any iterable spreads, not just arrays.
console.log("spread_string=" + firstLast(..."abc"));
console.log("spread_set=" + sum(...new Set([1, 2, 3])));
console.log("spread_map_values=" + sum(...new Map([["a", 4], ["b", 6]]).values()));
function* threeValues(): Generator<number> { yield 10; yield 20; yield 30; }
console.log("spread_generator=" + sum(...threeValues()));

// 7) Holes in a spread array become undefined arguments, so the count includes
//    them.
console.log("spread_holes=" + countArgs(...[1, , 3]));
console.log("spread_holes_values=" + firstLast(...[1, , 3]).length);

// 8) A non-iterable spread is a TypeError, and the fixed arguments before it
//    were already evaluated.
const evaluated: string[] = [];
function note<T>(label: string, v: T): T {
  evaluated.push(label);
  return v;
}
function spreadNonIterable(): string {
  try {
    return "ok:" + countArgs(note("fixed", 1), ...(note("bad", 42) as any));
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("non_iterable=" + spreadNonIterable());
console.log("non_iterable_order=" + evaluated.join(","));

// 9) Operands are evaluated left to right, spread sources included.
evaluated.length = 0;
countArgs(note("a", 1), ...(note("b", [2, 3]) as any), note("c", 4));
console.log("eval_order=" + evaluated.join(","));

// 10) The spread source is iterated once, at call time.
let nextCalls = 0;
const countingIterable: any = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        nextCalls += 1;
        return i < 3 ? { value: i++, done: false } : { value: undefined, done: true };
      },
    };
  },
};
console.log("counted_args=" + countArgs(...countingIterable));
console.log("next_calls=" + nextCalls);

// 11) `apply` does not iterate: it reads `length` and the indices.
let iterated = 0;
const arrayLike: any = {
  length: 3,
  0: "x",
  1: "y",
  2: "z",
  [Symbol.iterator]() {
    iterated += 1;
    return [][Symbol.iterator]();
  },
};
console.log("apply_arraylike=" + firstLast.apply(null, arrayLike));
console.log("apply_did_not_iterate=" + iterated);
console.log("spread_does_iterate=" + firstLast(...arrayLike) + "|iterated=" + iterated);

// 12) `new` accepts a spread too.
class Triple {
  joined: string;
  constructor(...parts: any[]) {
    this.joined = parts.join("-");
  }
}
console.log("new_spread=" + new Triple(...["p", "q", "r"]).joined);
console.log("new_spread_mixed=" + new Triple("head", ...["mid"], "tail").joined);
console.log("reflect_construct=" + Reflect.construct(Triple, many).joined.length);

// 13) A bound function prepends its own arguments to the spread ones.
const boundSum: any = sum.bind(null, 100, 200);
console.log("bound_then_spread=" + boundSum(...[1, 2, 3]));
console.log("bound_then_apply=" + boundSum.apply(null, [1, 2, 3]));
console.log("bound_length=" + boundSum.length);

// 14) A rest parameter collects everything past the fixed ones into a fresh
//     array, and that array is a real one.
function restShape(first: any, ...others: any[]): string {
  return "first=" + String(first) + " rest=" + others.length + " isArray=" + Array.isArray(others);
}
console.log("rest_shape=" + restShape(...many));
console.log("rest_shape_short=" + restShape(1));

// 15) `arguments.length` and the rest array agree.
function bothViews(first: any, ...others: any[]): string {
  return arguments.length + "/" + (others.length + 1);
}
console.log("both_views=" + bothViews(...many));

// 16) Spreading into `console.log` is avoided, but spreading into a builder
//     keeps the order intact across a large list.
console.log("order_preserved=" + (function (...values: number[]): boolean {
  for (let i = 0; i < values.length; i++) if (values[i] !== i + 1) return false;
  return values.length === 128;
})(...many));
