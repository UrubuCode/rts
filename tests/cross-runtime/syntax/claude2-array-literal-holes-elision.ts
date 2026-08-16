// Cross-runtime: elision in an array LITERAL. A missing element is a hole, not
// an `undefined` value — the index is absent — and a trailing comma adds
// nothing. Which operations see the difference is the assertion.

const holed: any[] = [1, , 3];
console.log("length=" + holed.length);
console.log("hole_value=" + String(holed[1]));
console.log("hole_in=" + (1 in holed) + "|filled_in=" + (0 in holed));
console.log("hole_hasOwn=" + Object.prototype.hasOwnProperty.call(holed, 1));

// 1) Trailing commas: the last one is elision-free, extra ones are holes.
console.log("trailing_one=" + [1, 2, ].length);
console.log("only_comma=" + [, ].length);
console.log("two_commas=" + [, , ].length);
console.log("empty_literal=" + [].length);
console.log("hole_then_value=" + [, 9].length + "|" + String([, 9][0]) + "|" + [, 9][1]);

// 2) Object.keys and the own-property names skip holes.
console.log("keys=" + Object.keys(holed).join(","));
console.log("own_names=" + Object.getOwnPropertyNames(holed).join(","));

// 3) Iteration methods skip holes; the callback is never called for one.
const visited: string[] = [];
holed.forEach((v, i) => { visited.push(i + ":" + String(v)); });
console.log("forEach_visits=" + visited.join(","));

const mapped = holed.map((v) => "seen" + String(v));
console.log("map_length=" + mapped.length + "|hole_kept=" + !(1 in mapped));
console.log("map_values=" + mapped.map((v) => String(v)).join(","));

console.log("filter_drops_holes=" + holed.filter(() => true).length);
console.log("some_skips=" + holed.some((v) => v === undefined));
console.log("every_skips=" + holed.every((v) => typeof v === "number"));

// 4) The methods that DO see a hole read it as undefined.
console.log("join=" + JSON.stringify(holed.join("-")));
console.log("toString=" + JSON.stringify(String(holed)));
console.log("includes_undefined=" + holed.includes(undefined));
console.log("indexOf_undefined=" + holed.indexOf(undefined));
console.log("find_index=" + holed.findIndex((v) => v === undefined));

// 5) Spread and for-of materialise holes as undefined values.
const spread = [...holed];
console.log("spread_length=" + spread.length + "|index_present=" + (1 in spread));
const fromIteration: string[] = [];
for (const v of holed) fromIteration.push(String(v));
console.log("for_of=" + fromIteration.join(","));
const converted = Array.from(holed);
console.log("array_from=" + converted.length + "|index_present=" + (1 in converted));

// 6) `for-in` walks index KEYS, so it skips the hole.
const inKeys: string[] = [];
for (const k in holed) inKeys.push(k);
console.log("for_in=" + inKeys.join(","));

// 7) JSON turns a hole into null.
console.log("json=" + JSON.stringify(holed));

// 8) Destructuring: a hole triggers the default, a stored undefined does too.
const [d0 = "D0", d1 = "D1", d2 = "D2"] = holed;
console.log("destructure_defaults=" + d0 + "," + d1 + "," + d2);
const [e0 = "E0"] = [undefined];
console.log("undefined_also_defaults=" + e0);

// 9) An elision in the PATTERN skips an element without naming it.
const [, second, , fourth = "F"] = [10, 20, 30];
console.log("pattern_elision=" + second + "," + fourth);

// 10) Assigning to the hole's index fills it; deleting an element makes one.
const filled: any[] = [1, , 3];
filled[1] = "now-here";
console.log("filled=" + (1 in filled) + "|" + filled[1]);
const deleted: any[] = [1, 2, 3];
delete deleted[1];
console.log("deleted_hole=" + (1 in deleted) + "|length=" + deleted.length);

// 11) Growing an array by `length` adds holes at the end.
const grown: any[] = [1];
grown.length = 3;
console.log("grown=" + grown.length + "|two_in=" + (2 in grown) + "|keys=" + Object.keys(grown).join(","));

// 12) `Array(n)` is all holes; `Array.of` and a literal are not.
const sized = new Array(3);
console.log("Array_n=" + sized.length + "|zero_in=" + (0 in sized) + "|keys=" + Object.keys(sized).length);
console.log("Array_of=" + Array.of(3).length + "|zero_in=" + (0 in Array.of(3)));

// 13) `fill` converts holes into real values.
const refilled = new Array(3).fill("x");
console.log("fill=" + refilled.join(",") + "|one_in=" + (1 in refilled));

// 14) `concat` keeps the hole from either side.
const joined = [1, , 3].concat([4, , 6]);
console.log("concat_length=" + joined.length + "|hole1=" + !(1 in joined) + "|hole4=" + !(4 in joined));

// 15) `slice` keeps holes in the copied range.
const sliced = [1, , 3, 4].slice(0, 3);
console.log("slice_length=" + sliced.length + "|hole=" + !(1 in sliced));

// 16) A nested literal's holes are independent of the outer array's.
const nested: any[] = [[1, , 3], , [, 2]];
console.log("nested_outer_hole=" + !(1 in nested) + "|inner0_hole=" + !(1 in nested[0]) +
  "|inner2_hole=" + !(0 in nested[2]));

// 17) The elision count is what decides the length, not the comma count: n
//     commas after the last value give n holes when nothing follows.
console.log("counts=" + [1].length + "," + [1, ].length + "," + [1, , ].length + "," + [1, , , ].length);

// 18) A spread of a holed array inside a literal fills the gaps with undefined
//     while a literal hole beside it stays a hole.
const mixed: any[] = [, ...[1, , 3], ];
console.log("mixed_length=" + mixed.length + "|first_hole=" + !(0 in mixed) +
  "|spread_hole_filled=" + (2 in mixed));

// 19) `flat` drops holes at every level.
console.log("flat=" + [1, , [2, , 3]].flat().length);

// 20) `reduce` skips holes, and an all-hole array with no initial value throws.
console.log("reduce=" + [1, , 3].reduce((a: any, b: any) => a + b, 0));
function reduceEmpty(): string {
  try {
    return String(new Array(3).reduce((a: any, b: any) => a + b));
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("reduce_all_holes=" + reduceEmpty());
