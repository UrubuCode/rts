// ONE thing: three different tests hide behind "no value". `||` asks FALSY,
// `??` and `?.` ask NULLISH (null or undefined), and a parameter or
// destructuring default asks UNDEFINED ONLY — so 0, "" and false pass one, two
// or all three of them depending on which was written.

function withDefault(a: any = "D"): string {
  return String(a);
}

function probe(label: string, v: any): void {
  const [destructured = "D"] = [v];
  const { p = "D" } = { p: v };
  console.log(
    label +
      " | falsy:" + !v +
      " | nullish:" + (v == null) +
      " | is_undefined:" + (v === undefined) +
      " | or:" + String(v || "D") +
      " | nullish_op:" + String(v ?? "D") +
      " | param:" + withDefault(v) +
      " | array_default:" + String(destructured) +
      " | object_default:" + String(p) +
      " | optional_chain:" + String(v?.constructor === undefined ? "skipped_or_none" : v.constructor.name)
  );
}

probe("undefined", undefined);
probe("null", null);
probe("false", false);
probe("zero", 0);
probe("neg_zero", -0);
probe("nan", NaN);
probe("empty_string", "");
probe("zero_string", "0");
probe("space", " ");
probe("zero_bigint", 0n);
probe("one", 1);
probe("empty_array", []);
probe("empty_object", {});
probe("false_wrapper", new Boolean(false));

// --- the three tests, tabulated as which values each one "replaces" ---
const values: [string, any][] = [
  ["undefined", undefined], ["null", null], ["false", false], ["0", 0],
  ["-0", -0], ["NaN", NaN], ["''", ""], ["0n", 0n], ["'0'", "0"], ["[]", []],
];
const byOr: string[] = [];
const byNullish: string[] = [];
const byDefault: string[] = [];
for (const v of values) {
  if ((v[1] || "D") === "D") byOr.push(v[0]);
  if ((v[1] ?? "D") === "D") byNullish.push(v[0]);
  if (withDefault(v[1]) === "D") byDefault.push(v[0]);
}
console.log("replaced_by_or=" + byOr.join(","));
console.log("replaced_by_nullish=" + byNullish.join(","));
console.log("replaced_by_param_default=" + byDefault.join(","));
console.log("nullish_equals_default=" + (byNullish.join(",") === byDefault.join(",")));

// --- a parameter default fires on undefined only, and an explicit undefined
//     argument is indistinguishable from a missing one ---
console.log("no_argument=" + withDefault());
console.log("explicit_undefined=" + withDefault(undefined));
console.log("explicit_null=" + withDefault(null));
function argCount(a: any = "D", ...rest: any[]): string {
  return a + ":" + rest.length + ":" + arguments.length;
}
console.log("arguments_length=" + argCount() + " / " + argCount(undefined) + " / " + argCount(null, 1));

// --- the default expression is evaluated LAZILY, once per missing argument ---
const evaluated: string[] = [];
function mark(tag: string): string {
  evaluated.push(tag);
  return "D" + tag;
}
function lazy(a: any = mark("a"), b: any = mark("b")): string {
  return String(a) + "/" + String(b);
}
console.log("lazy_both=" + lazy() + " evaluated=" + evaluated.join(","));
evaluated.length = 0;
console.log("lazy_first_given=" + lazy(0) + " evaluated=" + evaluated.join(","));
evaluated.length = 0;
console.log("lazy_null_given=" + lazy(null, null) + " evaluated=" + evaluated.join(","));
evaluated.length = 0;

// --- optional chaining short-circuits the WHOLE chain, including the key ---
const keyLog: string[] = [];
function key(name: string): string {
  keyLog.push(name);
  return "x";
}
const present: any = { x: { y: 5 }, fn: () => "called" };
const absent: any = null;
console.log("chain_present=" + String(present?.[key("present")]?.y));
console.log("chain_absent=" + String(absent?.[key("absent")]?.y));
console.log("key_evaluations=" + keyLog.join(","));
console.log("call_present=" + String(present.fn?.()));
console.log("call_missing=" + String(present.missing?.()));
console.log("chain_on_false=" + String((false as any)?.toString()));
console.log("chain_on_zero=" + String((0 as any)?.toFixed(1)));
console.log("chain_on_empty_string=" + String("" ?.length));
console.log("chain_on_nan=" + String(NaN?.toFixed(0)));

// --- ??= writes only when the target is nullish, ||= whenever it is falsy ---
function assignments(initial: any): string {
  const a: any = { v: initial };
  const b: any = { v: initial };
  const c: any = { v: initial };
  a.v ??= "N";
  b.v ||= "O";
  c.v &&= "A";
  return "??=" + String(a.v) + " ||=" + String(b.v) + " &&=" + String(c.v);
}
for (const v of values) {
  console.log("assign_" + v[0] + " " + assignments(v[1]));
}

// --- and the operand, not a boolean, is what comes out ---
console.log("or_types=" + typeof (0 || "s") + "," + typeof ("" || 0) + "," + typeof (null ?? 0n));
console.log("nullish_keeps_neg_zero=" + Object.is(-0 ?? 1, -0));
console.log("or_loses_neg_zero=" + Object.is(-0 || 1, -0) + " gives=" + String(-0 || 1));
console.log("nullish_keeps_nan=" + Number.isNaN(NaN ?? 1));
console.log("chained=" + String(null ?? undefined ?? 0 ?? 9) + "," + String(null || undefined || 0 || 9));
