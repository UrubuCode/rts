// ONE thing: &&, || and ?? are SELECTION operators, not boolean operators.
// They return one of the operands untouched — type included — and they leave
// the other one unevaluated, which for the logical assignment forms means the
// setter is never called at all.

function label(v: any): string {
  if (typeof v === "function") return "function";
  if (v === null) return "object:null";
  return typeof v + ":" + String(v);
}

// --- the result is an operand, not a boolean ---
console.log("or_falsy_left=" + label(0 || "a"));
console.log("or_truthy_left=" + label("a" || 0));
console.log("or_both_falsy=" + label("" || 0));
console.log("or_null_chain=" + label(null || undefined || NaN || 0 || "last"));
console.log("and_truthy_left=" + label(1 && "x"));
console.log("and_falsy_left=" + label(0 && "x"));
console.log("and_chain=" + label(1 && 2 && 3));
console.log("or_chain=" + label(1 || 2 || 3));
console.log("and_object=" + label(1 && { a: 1 } && "end"));
console.log("or_negzero=" + label(-0 || "replaced"));
console.log("and_nan=" + label(NaN && "never"));
console.log("or_emptyarray=" + label([] || "never"));
console.log("and_emptystring=" + label("" && "never"));

// --- ?? only looks at null and undefined, not at falsiness ---
console.log("nullish_null=" + label(null ?? "d"));
console.log("nullish_undefined=" + label(undefined ?? "d"));
console.log("nullish_zero=" + label(0 ?? "d"));
console.log("nullish_nan=" + label(NaN ?? "d"));
console.log("nullish_empty=" + label("" ?? "d"));
console.log("nullish_false=" + label(false ?? "d"));
console.log("nullish_negzero_is_neg0=" + Object.is(-0 ?? 1, -0));
console.log("or_vs_nullish=" + label(0 || "d") + " / " + label(0 ?? "d"));
console.log("parenthesised_mix=" + label((0 || null) ?? "d"));

// --- short circuit: the right operand is never evaluated ---
const log: string[] = [];
function note(name: string, value: any): any {
  log.push(name);
  return value;
}
note("a", 1) && note("b", 2);
note("c", 0) && note("d", 3);
note("e", 1) || note("f", 4);
note("g", 0) || note("h", 5);
note("i", null) ?? note("j", 6);
note("k", 0) ?? note("l", 7);
console.log("eval_log=" + log.join(","));

// --- the conditional operator evaluates exactly one branch ---
const clog: string[] = [];
function cnote(name: string): number {
  clog.push(name);
  return 1;
}
const picked = true ? cnote("then") : cnote("else");
const picked2 = false ? cnote("then2") : cnote("else2");
console.log("cond_log=" + clog.join(","));
console.log("cond_values=" + picked + "," + picked2);

// --- the logical ASSIGNMENT forms skip the write entirely, so no setter runs
const acc: string[] = [];
const obj: any = {
  _x: "truthy",
  _y: 0,
  _z: null,
  get x() { acc.push("get_x"); return this._x; },
  set x(v: any) { acc.push("set_x"); this._x = v; },
  get y() { acc.push("get_y"); return this._y; },
  set y(v: any) { acc.push("set_y"); this._y = v; },
  get z() { acc.push("get_z"); return this._z; },
  set z(v: any) { acc.push("set_z"); this._z = v; },
};
obj.x ||= "replaced";
obj.y ||= "replaced";
obj.z ??= "filled";
obj.x &&= "kept";
console.log("assign_log=" + acc.join(","));
console.log("final_x=" + label(obj._x));
console.log("final_y=" + label(obj._y));
console.log("final_z=" + label(obj._z));

const acc2: string[] = [];
const obj2: any = {
  _v: 1,
  get v() { acc2.push("get"); return this._v; },
  set v(n: any) { acc2.push("set"); this._v = n; },
};
obj2.v &&= 9;
obj2.v ??= 99;
console.log("assign_log2=" + acc2.join(","));
console.log("final_v=" + label(obj2._v));

// --- ! always produces a boolean; == does not ---
console.log("--- negation vs equality ---");
const probes: [string, any][] = [
  ["two_string", "2"],
  ["one_string", "1"],
  ["empty_array", []],
  ["array_one", [1]],
  ["space", " "],
  ["nan", NaN],
  ["null", null],
];
for (const p of probes) {
  const v: any = p[1];
  console.log(
    p[0] +
      " | !!:" + !!v +
      " | !!typeof:" + typeof !!v +
      " | ==true:" + (v == true) +
      " | ===true:" + (v === true) +
      " | !!==Boolean:" + (!!v === Boolean(v))
  );
}
console.log("double_negation_of_negzero=" + label(!!-0));
console.log("not_of_object=" + label(!{}));
console.log("de_morgan=" + (!(true && false) === (!true || !false)));
