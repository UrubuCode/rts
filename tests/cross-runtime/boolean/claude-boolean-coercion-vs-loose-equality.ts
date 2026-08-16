// ONE thing: truthiness and `== true` are DIFFERENT questions. ToBoolean has a
// closed list of seven falsy values; `x == true` first coerces true to 1 and
// then compares numerically — so "2" is truthy yet "2" == true is false, and
// new Boolean(false) is truthy yet equals false.

const table: [string, any][] = [
  ["false", false],
  ["true", true],
  ["zero", 0],
  ["negzero", -0],
  ["one", 1],
  ["neg_one", -1],
  ["tenth", 0.1],
  ["nan", NaN],
  ["infinity", Infinity],
  ["neg_infinity", -Infinity],
  ["zero_bigint", 0n],
  ["one_bigint", 1n],
  ["neg_one_bigint", -1n],
  ["empty_string", ""],
  ["space_string", " "],
  ["zero_string", "0"],
  ["false_string", "false"],
  ["two_string", "2"],
  ["one_string", "1"],
  ["null", null],
  ["undefined", undefined],
  ["empty_array", []],
  ["array_zero", [0]],
  ["array_one", [1]],
  ["array_two_elems", [1, 2]],
  ["array_empty_nested", [[]]],
  ["array_empty_string", [""]],
  ["empty_object", {}],
  ["function", function () { return 0; }],
  ["boolean_false_wrapper", new Boolean(false)],
  ["number_zero_wrapper", new Number(0)],
  ["number_nan_wrapper", new Number(NaN)],
  ["string_empty_wrapper", new String("")],
  ["bigint_zero_wrapper", Object(0n)],
  ["symbol", Symbol("s")],
  ["symbol_wrapper", Object(Symbol("s"))],
  ["object_null_proto", Object.create(null)],
  ["date_epoch", new Date(0)],
];

for (const row of table) {
  const label = row[0];
  const v: any = row[1];
  let eqTrue = "";
  let eqFalse = "";
  let eqOne = "";
  let eqZero = "";
  try { eqTrue = String(v == true); } catch (e) { eqTrue = "!" + (e as any).constructor.name; }
  try { eqFalse = String(v == false); } catch (e) { eqFalse = "!" + (e as any).constructor.name; }
  try { eqOne = String(v == 1); } catch (e) { eqOne = "!" + (e as any).constructor.name; }
  try { eqZero = String(v == 0); } catch (e) { eqZero = "!" + (e as any).constructor.name; }
  console.log(
    label +
      " | Boolean:" + Boolean(v) +
      " | !!:" + !!v +
      " | ternary:" + (v ? "T" : "F") +
      " | ==true:" + eqTrue +
      " | ==false:" + eqFalse +
      " | ==1:" + eqOne +
      " | ==0:" + eqZero
  );
}

// --- the three spellings of truthiness always agree with each other ---
console.log("--- the three spellings agree ---");
let allAgree = true;
for (const row of table) {
  const v: any = row[1];
  const a = Boolean(v);
  const b = !!v;
  const c = v ? true : false;
  const d = !!(v && true) === (a && true);
  if (a !== b || b !== c || !d) {
    allAgree = false;
    console.log("disagreement_at=" + row[0]);
  }
}
console.log("all_agree=" + allAgree);

// --- the falsy list is exactly seven long ---
console.log("--- the falsy list ---");
const falsy = table.filter(function (row) { return !row[1]; }).map(function (row) { return row[0]; });
console.log("falsy=" + falsy.join(","));
console.log("falsy_count=" + falsy.length);

// --- Boolean as a constructor vs as a function ---
console.log("--- constructor vs function ---");
console.log("call_typeof=" + typeof Boolean(false));
console.log("new_typeof=" + typeof new Boolean(false));
console.log("new_truthy=" + (new Boolean(false) ? "T" : "F"));
console.log("new_valueOf=" + new Boolean(false).valueOf());
console.log("new_eq_false=" + (new Boolean(false) == false));
console.log("new_strict_eq_false=" + ((new Boolean(false) as any) === false));
console.log("new_eq_new=" + (new Boolean(false) == new Boolean(false)));
console.log("new_tag=" + Object.prototype.toString.call(new Boolean(false)));
console.log("no_args=" + Boolean());
console.log("length=" + Boolean.length);
console.log("proto_valueOf=" + Boolean.prototype.valueOf());
console.log("proto_toString=" + Boolean.prototype.toString());
console.log("proto_typeof=" + typeof Boolean.prototype);

// --- the brand check on Boolean.prototype.valueOf ---
function call(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}
call("valueOf_prim", () => Boolean.prototype.valueOf.call(true));
call("valueOf_wrapper", () => Boolean.prototype.valueOf.call(new Boolean(true)));
call("valueOf_number", () => Boolean.prototype.valueOf.call(1 as any));
call("valueOf_object", () => Boolean.prototype.valueOf.call({} as any));
call("toString_prim", () => Boolean.prototype.toString.call(false));
