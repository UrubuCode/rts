// Cross-runtime: a typed array sorts NUMERICALLY by default (an Array sorts by
// string), and the change-by-copy trio toSorted/toReversed/with answers a new
// typed array of the same kind while leaving the receiver alone.

const nums = [10, 9, 2, 100, 1];
console.log("array_default=" + nums.slice().sort().join(","));
console.log("typed_default=" + Array.from(new Int32Array(nums).sort()).join(","));
console.log("typed_negatives=" + Array.from(new Int32Array([3, -10, 0, -1]).sort()).join(","));
console.log("typed_comparator=" + Array.from(new Int32Array(nums).sort(function (a, b) { return b - a; })).join(","));

// sort() mutates and answers the same object.
const inPlace = new Uint8Array([3, 1, 2]);
console.log("sort_identity=" + (inPlace.sort() === inPlace) + " " + Array.from(inPlace).join(","));

// Float ordering: -0 before +0, NaN last, infinities at the ends.
const floats = new Float64Array([3, NaN, -0, 0, -1, Infinity, -Infinity, NaN]);
floats.sort();
const shown: string[] = [];
for (let i = 0; i < floats.length; i++) {
  shown.push(Object.is(floats[i], -0) ? "-0" : String(floats[i]));
}
console.log("float_sort=" + shown.join(","));

// The comparator is called with numbers, and a NaN result is treated as 0.
let calls = 0;
const counted = new Uint8Array([2, 1, 3]);
counted.sort(function (a, b) {
  calls++;
  return typeof a === "number" && typeof b === "number" ? a - b : 0;
});
console.log("comparator_types=" + Array.from(counted).join(",") + " called=" + (calls > 0));
console.log("comparator_nan=" + Array.from(new Uint8Array([2, 1, 3]).sort(function () { return NaN; })).join(","));

// Sorting a view sorts only the view's window.
const whole = new Uint8Array([5, 4, 3, 2, 1]);
whole.subarray(1, 4).sort();
console.log("view_sort=" + Array.from(whole).join(","));

// toSorted / toReversed / with copy, keeping the element kind.
const src = new Int8Array([3, 1, 2]);
const sorted = src.toSorted();
console.log("toSorted=" + Array.from(sorted).join(",") + " kind=" + sorted.constructor.name);
console.log("toSorted_src=" + Array.from(src).join(",") + " same=" + ((sorted as any) === (src as any)));
console.log("toSorted_cmp=" + Array.from(src.toSorted(function (a, b) { return b - a; })).join(","));
const reversed = src.toReversed();
console.log("toReversed=" + Array.from(reversed).join(",") + " kind=" + reversed.constructor.name);
console.log("toReversed_src=" + Array.from(src).join(","));

// with() coerces the value the same way an element write does, and rejects an
// index outside the range instead of ignoring it.
const withHigh = src.with(1, 300);
console.log("with=" + Array.from(withHigh).join(",") + " kind=" + withHigh.constructor.name);
console.log("with_src=" + Array.from(src).join(","));
console.log("with_neg=" + Array.from(src.with(-1, 7)).join(","));
console.log("with_float_idx=" + Array.from(src.with(1.9, 7)).join(","));
try {
  src.with(3, 1);
  console.log("with_oob=no-throw");
} catch (e: any) {
  console.log("with_oob=" + e.constructor.name);
}
try {
  src.with(-4, 1);
  console.log("with_neg_oob=no-throw");
} catch (e: any) {
  console.log("with_neg_oob=" + e.constructor.name);
}
try {
  new BigInt64Array(2).with(0, 1 as any);
  console.log("with_number_into_bigint=no-throw");
} catch (e: any) {
  console.log("with_number_into_bigint=" + e.constructor.name);
}

// A copy never shares the receiver's buffer.
console.log("copies_detached_from_buffer=" + (sorted.buffer !== src.buffer) + "," + (reversed.buffer !== src.buffer) + "," + (withHigh.buffer !== src.buffer));

// at() indexes from the end and answers undefined past the range.
const at = new Uint16Array([7, 8, 9]);
console.log("at=" + at.at(0) + "," + at.at(-1) + "," + String(at.at(3)) + "," + String(at.at(-4)));
console.log("at_float=" + String(at.at(1.9)) + "," + String(at.at(NaN)));

// Reverse and its copying sibling on the same data.
const rev = new Uint8Array([1, 2, 3]);
console.log("reverse_identity=" + (rev.reverse() === rev) + " " + Array.from(rev).join(","));
