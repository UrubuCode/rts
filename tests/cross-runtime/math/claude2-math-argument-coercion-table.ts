// ONE thing: every Math method runs ToNumber on its arguments first, so an
// object reaches it only through @@toPrimitive/valueOf/toString — and a
// two-argument method coerces LEFT to RIGHT, before it computes anything, which
// a throwing second argument makes visible.

const args: [string, any][] = [
  ["undefined", undefined],
  ["null", null],
  ["true", true],
  ["false", false],
  ["empty_string", ""],
  ["space_string", "   "],
  ["num_string", "-4.5"],
  ["hex_string", "0x10"],
  ["bin_string", "0b101"],
  ["inf_string", "Infinity"],
  ["bad_string", "12abc"],
  ["empty_array", []],
  ["one_array", [9]],
  ["two_array", [1, 2]],
  ["plain_object", {}],
  ["date_like", { valueOf: () => 7.5 }],
  ["string_only", { toString: () => "-3" }],
];

function row(name: string, fn: (x: number) => number): void {
  const cells: string[] = [];
  for (const pair of args) {
    const r = fn(pair[1] as any);
    cells.push(pair[0] + ":" + String(r) + (Object.is(r, -0) ? "(-0)" : ""));
  }
  console.log(name + " | " + cells.join(" "));
}

row("abs", Math.abs);
row("sign", Math.sign);
row("floor", Math.floor);
row("ceil", Math.ceil);
row("round", Math.round);
row("trunc", Math.trunc);
row("sqrt", Math.sqrt);
row("clz32", Math.clz32);
row("fround", Math.fround);

// --- @@toPrimitive wins over valueOf, and the hint is "number" ---
const hinted: any = {
  [Symbol.toPrimitive](hint: string) {
    hints.push(hint);
    return 12.7;
  },
  valueOf() {
    hints.push("valueOf");
    return 0;
  },
};
const hints: string[] = [];
console.log("toPrimitive_floor=" + String(Math.floor(hinted)));
console.log("toPrimitive_abs=" + String(Math.abs(hinted)));
console.log("hints=" + hints.join(","));

// --- valueOf is consulted once per argument, in order ---
const order: string[] = [];
function tracked(name: string, value: number): any {
  return {
    valueOf() {
      order.push(name);
      return value;
    },
  };
}
console.log("pow=" + String(Math.pow(tracked("base", 2), tracked("exp", 10))));
console.log("pow_order=" + order.join(","));

order.length = 0;
console.log("atan2_sign=" + (Math.atan2(tracked("y", 1), tracked("x", 1)) > 0));
console.log("atan2_order=" + order.join(","));

order.length = 0;
console.log("imul=" + String(Math.imul(tracked("a", 3), tracked("b", 4))));
console.log("imul_order=" + order.join(","));

order.length = 0;
console.log("hypot=" + String(Math.hypot(tracked("h1", 3), tracked("h2", 4))));
console.log("hypot_order=" + order.join(","));

// --- a throwing argument aborts before any result, even in last position ---
const boom: any = {
  valueOf() {
    throw new RangeError("no");
  },
};
function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}
attempt("pow_throwing_exponent", () => Math.pow(2, boom));
attempt("pow_throwing_base", () => Math.pow(boom, 2));
attempt("atan2_throwing_second", () => Math.atan2(1, boom));
attempt("hypot_throwing_third", () => Math.hypot(3, 4, boom));
attempt("abs_throwing", () => Math.abs(boom));
attempt("imul_throwing_second", () => Math.imul(1, boom));

// --- the first argument is still coerced when the second throws ---
order.length = 0;
try {
  Math.pow(tracked("first", 2), boom);
} catch (e) {
  order.push("threw:" + (e as any).constructor.name);
}
console.log("coerced_before_throw=" + order.join(","));

// --- a BigInt cannot be coerced by ToNumber at all ---
attempt("abs_bigint", () => Math.abs(1n as any));
attempt("floor_bigint", () => Math.floor(1n as any));
attempt("max_bigint", () => Math.max(1n as any, 2 as any));
attempt("clz32_bigint", () => Math.clz32(1n as any));

// --- and neither can a Symbol ---
attempt("abs_symbol", () => Math.abs(Symbol("s") as any));
attempt("trunc_symbol", () => Math.trunc(Symbol("s") as any));

// --- extra arguments are ignored by a unary method ---
console.log("abs_extra_args=" + String((Math.abs as any)(-1, boom)));
console.log("sqrt_extra_args=" + String((Math.sqrt as any)(16, boom)));
