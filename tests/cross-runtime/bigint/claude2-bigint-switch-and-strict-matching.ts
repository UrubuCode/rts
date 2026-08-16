// ONE thing: every construct that matches by STRICT equality keeps 1n and 1
// apart — switch, includes, indexOf, find and the ternary chains built on ===.
// A BigInt only meets a Number under ==, which switch never uses, so a numeric
// case can never catch a BigInt however equal the two look when printed.

function classify(x: any): string {
  switch (x) {
    case 1:
      return "number_one";
    case 1n:
      return "bigint_one";
    case "1":
      return "string_one";
    case true:
      return "true";
    case 0:
      return "number_zero";
    case 0n:
      return "bigint_zero";
    default:
      return "default";
  }
}

const probes: [string, any][] = [
  ["1n", 1n],
  ["1", 1],
  ["'1'", "1"],
  ["true", true],
  ["0n", 0n],
  ["0", 0],
  ["-0", -0],
  ["-0n", -0n],
  ["computed_1n", 0n + 1n],
  ["computed_1", 0 + 1],
  ["BigInt(1)", BigInt(1)],
  ["Number(1n)", Number(1n)],
  ["wrapper_1n", Object(1n)],
  ["2n", 2n],
  ["1.0", 1.0],
];
for (const p of probes) {
  console.log("switch(" + p[0] + ")=" + classify(p[1]));
}

// --- -0n does not exist: the literal is 0n, and negating it stays 0n ---
console.log("neg_zero_bigint=" + String(-0n) + " is_zero:" + (-0n === 0n));
console.log("neg_zero_number=" + Object.is(-0, -0) + " bigint_has_no_neg_zero=" + Object.is(-0n, 0n));

// --- the equality relations, side by side ---
const pairs: [string, any, any][] = [
  ["1n_vs_1", 1n, 1],
  ["1n_vs_1n", 1n, 1n],
  ["1n_vs_'1'", 1n, "1"],
  ["1n_vs_true", 1n, true],
  ["0n_vs_false", 0n, false],
  ["0n_vs_''", 0n, ""],
  ["0n_vs_'0'", 0n, "0"],
  ["1n_vs_1.5", 1n, 1.5],
  ["2n_vs_2.0", 2n, 2.0],
  ["1n_vs_NaN", 1n, NaN],
  ["1n_vs_null", 1n, null],
  ["1n_vs_undefined", 1n, undefined],
  ["1n_vs_[1]", 1n, [1]],
  ["1n_vs_wrapper", 1n, Object(1n)],
];
for (const p of pairs) {
  console.log(
    p[0] +
      " | loose:" + ((p[1] as any) == (p[2] as any)) +
      " | strict:" + ((p[1] as any) === (p[2] as any)) +
      " | Object.is:" + Object.is(p[1], p[2]) +
      " | includes:" + [p[1]].includes(p[2] as any)
  );
}

// --- searching an array of mixed numeric kinds ---
const mixed: any[] = [1, 1n, 2, 2n, "1", true];
console.log("indexOf_1=" + mixed.indexOf(1) + " indexOf_1n=" + mixed.indexOf(1n));
console.log("includes_2n=" + mixed.includes(2n) + " includes_2=" + mixed.includes(2));
console.log("find_bigint=" + String(mixed.find((v) => typeof v === "bigint")));
console.log("filter_bigint=" + mixed.filter((v) => typeof v === "bigint").map((v) => String(v)).join(","));
console.log("lastIndexOf_1n=" + mixed.lastIndexOf(1n));
console.log("count_loose_one=" + mixed.filter((v) => v == 1).length);
console.log("count_strict_one=" + mixed.filter((v) => v === 1).length);

// --- a switch falls through until a break, BigInt cases included ---
function collect(x: any): string {
  const hits: string[] = [];
  switch (x) {
    case 1n:
      hits.push("one");
    case 2n:
      hits.push("two");
      break;
    case 3n:
      hits.push("three");
      break;
    default:
      hits.push("none");
  }
  return hits.join(">");
}
console.log("fallthrough_1n=" + collect(1n));
console.log("fallthrough_2n=" + collect(2n));
console.log("fallthrough_3n=" + collect(3n));
console.log("fallthrough_1=" + collect(1));

// --- switch(true) DOES reach the mixed comparison, because the case
//     expressions are evaluated as ordinary relational operators ---
function bucket(x: bigint): string {
  switch (true) {
    case x < 0n:
      return "negative";
    case x === 0n:
      return "zero";
    case x < 10:
      return "small";
    case x > 1e10:
      return "huge";
    default:
      return "medium";
  }
}
console.log("bucket=" + [(-5n), 0n, 3n, 100n, 99999999999n].map((v) => bucket(v)).join(","));

// --- and the case expressions are evaluated in order, only until a match ---
const order: string[] = [];
function caseValue(name: string, v: any): any {
  order.push(name);
  return v;
}
switch (2n) {
  case caseValue("a", 1n):
    break;
  case caseValue("b", 2n):
    break;
  case caseValue("c", 3n):
    break;
}
console.log("case_evaluation_order=" + order.join(","));

// --- typeof and truthiness, which no coercion touches ---
console.log("typeof=" + typeof 1n + "," + typeof 0n + "," + typeof Object(1n));
console.log("truthy=" + (0n ? "t" : "f") + "," + (1n ? "t" : "f") + "," + (Object(0n) ? "t" : "f"));
console.log("boolean_of=" + Boolean(0n) + "," + Boolean(-1n) + "," + Boolean(Object(0n)));
