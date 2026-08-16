// ONE thing: ++ and -- coerce with ToNumeric and the POSTFIX form returns the
// coerced old value, not the original operand. So `let s = "5"; s++` yields the
// number 5 and leaves 6 behind — the string never reaches the result.

function label(v: any): string {
  return typeof v + ":" + String(v);
}

// --- postfix returns ToNumeric(old), so the type changes before you see it ---
let s: any = "5";
const sOld = s++;
console.log("str_post_old=" + label(sOld));
console.log("str_post_new=" + label(s));

let s2: any = "5";
const s2New = ++s2;
console.log("str_pre_result=" + label(s2New));
console.log("str_pre_var=" + label(s2));

// --- every falsy-ish operand goes through ToNumber first ---
const cases: [string, any][] = [
  ["empty_str", ""],
  ["space_str", "  "],
  ["hex_str", "0x10"],
  ["exp_str", "1e3"],
  ["bad_str", "abc"],
  ["true", true],
  ["false", false],
  ["null", null],
  ["undefined", undefined],
  ["emptyarr", []],
  ["arr7", [7]],
  ["arr_two", [1, 2]],
  ["obj", {}],
  ["negzero", -0],
  ["infinity", Infinity],
  ["nan", NaN],
];
for (const c of cases) {
  let inc: any = c[1];
  const incOld = inc++;
  let dec: any = c[1];
  const decOld = dec--;
  console.log(
    c[0] +
      " | ++old:" + label(incOld) +
      " | ++new:" + label(inc) +
      " | --old:" + label(decOld) +
      " | --new:" + label(dec)
  );
}

// --- valueOf is consulted exactly once, and the result is a plain number ---
const order: string[] = [];
let box: any = {
  valueOf: function () {
    order.push("valueOf");
    return 41;
  },
  toString: function () {
    order.push("toString");
    return "999";
  },
};
const boxOld = box++;
console.log("box_old=" + label(boxOld));
console.log("box_new=" + label(box));
console.log("box_order=" + order.join(","));

// --- Symbol.toPrimitive wins, and it is called with hint "number" ---
const hints: string[] = [];
let prim: any = {
  [Symbol.toPrimitive]: function (hint: string) {
    hints.push(hint);
    return 7;
  },
};
prim++;
console.log("prim_hint=" + hints.join(","));
console.log("prim_new=" + label(prim));

// --- the sign of zero: 0-- is -1, but 1-- lands on +0, not -0 ---
let z: any = 1;
z--;
console.log("one_dec_isNeg0=" + Object.is(z, -0));
let nz: any = -0;
const nzOld = nz++;
console.log("negzero_post_old_isNeg0=" + Object.is(nzOld, -0));
console.log("negzero_post_new=" + label(nz));
let nz2: any = 0;
nz2--;
console.log("zero_dec=" + label(nz2));

// --- at the top of the safe range ++ stops moving ---
let big: any = Number.MAX_SAFE_INTEGER;
big++;
console.log("maxsafe_plus1=" + label(big));
big++;
console.log("maxsafe_plus2=" + label(big));
console.log("maxsafe_plus2_isSafe=" + Number.isSafeInteger(big));

// --- ++ on an accessor property reads once and writes once ---
const reads: string[] = [];
const acc: any = {
  _v: 10,
  get v() {
    reads.push("get");
    return this._v;
  },
  set v(n: number) {
    reads.push("set:" + n);
    this._v = n;
  },
};
acc.v++;
console.log("accessor_log=" + reads.join(","));
console.log("accessor_final=" + label(acc._v));
