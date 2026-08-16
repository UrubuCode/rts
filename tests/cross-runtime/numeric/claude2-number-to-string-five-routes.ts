// ONE thing: the routes from a Number to a String, side by side. String(),
// .toString(), a template, `+ ""`, Array#join and the wrapper all run the same
// Number::toString — JSON.stringify does NOT: it answers "null" for every
// non-finite value, which is the one route that loses information.

const values: [string, number][] = [
  ["zero", 0],
  ["neg_zero", -0],
  ["one", 1],
  ["neg_one", -1],
  ["nan", NaN],
  ["infinity", Infinity],
  ["neg_infinity", -Infinity],
  ["tenth", 0.1],
  ["classic_sum", 0.1 + 0.2],
  ["1e20", 1e20],
  ["1e21", 1e21],
  ["neg_1e21", -1e21],
  ["1e-6", 1e-6],
  ["1e-7", 1e-7],
  ["neg_1e-7", -1e-7],
  ["max_value", Number.MAX_VALUE],
  ["min_value", Number.MIN_VALUE],
  ["max_safe", Number.MAX_SAFE_INTEGER],
  ["epsilon", Number.EPSILON],
  ["two_p53", 2 ** 53],
  ["big_int_like", 123456789012345680000],
  ["long_fraction", 123456789.123456789],
  ["exact_half", 2.5],
];

for (const pair of values) {
  const x = pair[1];
  console.log(
    pair[0] +
      " | String:" + String(x) +
      " | toString:" + x.toString() +
      " | template:" + `${x}` +
      " | plus:" + (x + "") +
      " | join:" + [x].join("") +
      " | concat:" + "".concat(x as any) +
      " | json:" + String(JSON.stringify(x))
  );
}

// --- all the non-JSON routes agree on every value above ---
const disagree: string[] = [];
for (const pair of values) {
  const x = pair[1];
  const a = String(x);
  if (x.toString() !== a || `${x}` !== a || x + "" !== a || [x].join("") !== a) {
    disagree.push(pair[0]);
  }
}
console.log("route_disagreement=[" + disagree.join(",") + "]");

// --- and JSON is the one that differs, on exactly the non-finite values ---
const jsonDiff: string[] = [];
for (const pair of values) {
  if (JSON.stringify(pair[1]) !== String(pair[1])) jsonDiff.push(pair[0]);
}
console.log("json_differs_on=[" + jsonDiff.join(",") + "]");

// --- the wrapper takes the same route, through valueOf then toString ---
console.log("wrapper_String=" + String(new Number(0.1 + 0.2)));
console.log("wrapper_plus=" + (new Number(-0) + ""));
console.log("wrapper_template=" + `${new Number(1e21)}`);
console.log("wrapper_json=" + String(JSON.stringify(new Number(NaN))));
console.log("wrapper_json_finite=" + String(JSON.stringify(new Number(2.5))));
console.log("Object_neg_zero=" + String(Object(-0)));

// --- toString(10) is not a different function from toString() ---
const radixDiff: string[] = [];
for (const pair of values) {
  if (pair[1].toString(10) !== pair[1].toString()) radixDiff.push(pair[0]);
}
console.log("radix10_differs=[" + radixDiff.join(",") + "]");
console.log("undefined_radix=" + (2.5).toString(undefined));

// --- join turns null and undefined into "", but never a number ---
console.log("join_mixed=" + [1, NaN, null, undefined, Infinity, -0, 3].join(","));
console.log("join_nested=" + String([1, [2, [3, NaN]]] + ""));

// --- a number inside a string operation is converted first ---
console.log("repeat_arg=" + "ab".repeat(2 as any));
console.log("string_plus_number=" + ("v" + 1e21));
console.log("number_plus_string=" + (1e-7 + "s"));
console.log("string_compare=" + ("10" < "9") + "," + (10 < 9));
console.log("padStart=" + String(1e21).padStart(8, "0"));
