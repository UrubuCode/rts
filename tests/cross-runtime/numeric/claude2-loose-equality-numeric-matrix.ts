// ONE thing: the FULL == matrix over the number-ish values, printed as a grid,
// beside the four other equality relations (===, Object.is, SameValueZero via
// includes, and the relational <=/>= pair). == is not transitive and not a
// refinement of ===; the grid is where that stops being an anecdote.

const wrapper: any = new Number(0);
const valued: any = { valueOf: () => 1 };

const rows: [string, any][] = [
  ["0", 0],
  ["-0", -0],
  ["1", 1],
  ["NaN", NaN],
  ["Inf", Infinity],
  ["''", ""],
  ["' '", " "],
  ["'0'", "0"],
  ["'1'", "1"],
  ["'0.0'", "0.0"],
  ["'Inf'", "Infinity"],
  ["false", false],
  ["true", true],
  ["null", null],
  ["undef", undefined],
  ["[]", []],
  ["[0]", [0]],
  ["[1]", [1]],
  ["{}", {}],
  ["0n", 0n],
  ["1n", 1n],
  ["Num0", wrapper],
  ["valueOf1", valued],
];

// --- the == grid, one row per left operand ---
console.log("legend=" + rows.map((r) => r[0]).join(" "));
for (const left of rows) {
  const cells: string[] = [];
  for (const right of rows) {
    cells.push((left[1] as any) == (right[1] as any) ? "T" : ".");
  }
  console.log("eq  " + left[0].padEnd(9, " ") + cells.join(""));
}

// --- the === grid, for the same operands ---
for (const left of rows) {
  const cells: string[] = [];
  for (const right of rows) {
    cells.push((left[1] as any) === (right[1] as any) ? "T" : ".");
  }
  console.log("id  " + left[0].padEnd(9, " ") + cells.join(""));
}

// --- every pair where == and === disagree ---
const disagree: string[] = [];
for (const left of rows) {
  for (const right of rows) {
    if (((left[1] as any) == (right[1] as any)) !== ((left[1] as any) === (right[1] as any))) {
      disagree.push(left[0] + "==" + right[0]);
    }
  }
}
console.log("loose_only=" + disagree.join(" "));

// --- and every pair where Object.is disagrees with === ---
const sameValue: string[] = [];
for (const left of rows) {
  for (const right of rows) {
    if (Object.is(left[1], right[1]) !== ((left[1] as any) === (right[1] as any))) {
      sameValue.push(left[0] + "/" + right[0]);
    }
  }
}
console.log("object_is_differs=" + sameValue.join(" "));

// --- SameValueZero, as Array#includes sees it: it splits -0 from Object.is
//     and joins NaN to itself ---
const szDiff: string[] = [];
for (const left of rows) {
  for (const right of rows) {
    const sz = [left[1]].includes(right[1] as any);
    if (sz !== Object.is(left[1], right[1])) szDiff.push(left[0] + "/" + right[0]);
  }
}
console.log("samevaluezero_differs_from_object_is=" + szDiff.join(" "));

// --- indexOf uses strict equality, so it loses NaN where includes keeps it ---
console.log("indexOf_nan=" + [NaN].indexOf(NaN) + " includes_nan=" + [NaN].includes(NaN));
console.log("indexOf_negzero=" + [-0].indexOf(0) + " includes_negzero=" + [-0].includes(0));
console.log("set_nan_size=" + new Set([NaN, NaN, 0 / 0]).size);
console.log("map_nan_get=" + String(new Map([[NaN, "v"]]).get(NaN)));

// --- transitivity fails: a == b and b == c without a == c ---
console.log("transitivity_1=" + ("0" == false) + "," + (false == []) + "," + ("0" == ([] as any)));
console.log("transitivity_2=" + (0 == "") + "," + ("" == "0") + "," + (0 == "0"));
console.log("transitivity_3=" + (1 == "1") + "," + ("1" == true) + "," + (1 == true));

// --- <= and >= are not the negation of > and <, because of NaN and coercion ---
const relRows: [string, any][] = [
  ["null", null], ["undef", undefined], ["0", 0], ["NaN", NaN], ["''", ""], ["[]", []],
];
for (const left of relRows) {
  const cells: string[] = [];
  for (const right of relRows) {
    const a: any = left[1];
    const b: any = right[1];
    cells.push(
      (a < b ? "<" : "") + (a > b ? ">" : "") + (a <= b ? "l" : "") + (a >= b ? "g" : "") + (a == b ? "=" : "") || "-"
    );
  }
  console.log("rel " + left[0].padEnd(6, " ") + cells.join(" "));
}
console.log("null_ge_zero=" + (null >= 0) + " null_gt_zero=" + (null > 0) + " null_eq_zero=" + ((null as any) == 0));
console.log("undef_ge_zero=" + ((undefined as any) >= 0) + " undef_eq_null=" + ((undefined as any) == null));
