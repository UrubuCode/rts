// Cross-runtime: repeat's count is ToInteger'd (truncated toward zero) and then
// checked against TWO different failures — a RangeError for negative/Infinity
// counts, and a RangeError for a result past the implementation's string limit —
// and the RECEIVER is coerced BEFORE the count, which a throwing toString can
// observe. 133/218/codex2_027 only pass valid counts.

function attempt(f: () => any): string {
  try {
    const v = f();
    return typeof v === "string" ? JSON.stringify(v) : String(v);
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- ToInteger truncates toward zero, it does not round ---
console.log("frac-low=" + attempt(() => "ab".repeat(2.1)));
console.log("frac-high=" + attempt(() => "ab".repeat(2.9)));
console.log("just-under-1=" + attempt(() => "ab".repeat(0.9)));
console.log("exactly-0=" + attempt(() => "ab".repeat(0)));
console.log("neg-zero=" + attempt(() => "ab".repeat(-0)));
console.log("neg-frac=" + attempt(() => "ab".repeat(-0.9)));
console.log("neg-one=" + attempt(() => "ab".repeat(-1)));
console.log("neg-tiny=" + attempt(() => "ab".repeat(-1e-9)));

// --- non-numbers go through ToNumber first ---
console.log("string=" + attempt(() => "ab".repeat("3" as any)));
console.log("string-frac=" + attempt(() => "ab".repeat("2.7" as any)));
console.log("string-empty=" + attempt(() => "ab".repeat("" as any)));
console.log("string-space=" + attempt(() => "ab".repeat("  " as any)));
console.log("string-bad=" + attempt(() => "ab".repeat("x" as any)));
console.log("string-hex=" + attempt(() => "ab".repeat("0x2" as any)));
console.log("null=" + attempt(() => "ab".repeat(null as any)));
console.log("undefined=" + attempt(() => "ab".repeat(undefined as any)));
console.log("no-arg=" + attempt(() => ("ab" as any).repeat()));
console.log("true=" + attempt(() => "ab".repeat(true as any)));
console.log("false=" + attempt(() => "ab".repeat(false as any)));
console.log("nan=" + attempt(() => "ab".repeat(NaN)));
console.log("array=" + attempt(() => "ab".repeat([2] as any)));
console.log("array-empty=" + attempt(() => "ab".repeat([] as any)));
console.log("array-two=" + attempt(() => "ab".repeat([1, 2] as any)));
console.log("object=" + attempt(() => "ab".repeat({ valueOf: () => 2 } as any)));
console.log("symbol=" + attempt(() => "ab".repeat(Symbol("2") as any)));
console.log("bigint=" + attempt(() => "ab".repeat(2n as any)));

// --- the two RangeError families ---
console.log("infinity=" + attempt(() => "ab".repeat(Infinity)));
console.log("neg-infinity=" + attempt(() => "ab".repeat(-Infinity)));
console.log("over-limit=" + attempt(() => "ab".repeat(1e10).length));
console.log("over-limit-2p31=" + attempt(() => "ab".repeat(2147483648).length));
console.log("max-safe=" + attempt(() => "ab".repeat(Number.MAX_SAFE_INTEGER).length));

// --- an EMPTY receiver escapes the size check but not the range check ---
console.log("empty-huge=" + attempt(() => "".repeat(1e10).length));
console.log("empty-max=" + attempt(() => "".repeat(Number.MAX_SAFE_INTEGER).length));
console.log("empty-infinity=" + attempt(() => "".repeat(Infinity)));
console.log("empty-neg=" + attempt(() => "".repeat(-1)));
console.log("empty-zero=" + attempt(() => "".repeat(0)));

// --- and a zero count escapes the size check for any receiver ---
console.log("long-zero=" + attempt(() => "x".repeat(1000).repeat(0).length));

// --- ORDER: the receiver is coerced first, so its throw wins ---
const order: string[] = [];
const badThis: any = { toString: () => { order.push("this"); throw new RangeError("this"); } };
const badCount: any = { valueOf: () => { order.push("count"); throw new TypeError("count"); } };
console.log("both-throw=" + attempt(() => String.prototype.repeat.call(badThis, badCount)));
console.log("order=" + order.join(","));

order.length = 0;
const okThis: any = { toString: () => { order.push("this"); return "ab"; } };
console.log("count-throws=" + attempt(() => String.prototype.repeat.call(okThis, badCount)));
console.log("order2=" + order.join(","));

order.length = 0;
const okCount: any = { valueOf: () => { order.push("count"); return 2; } };
console.log("this-throws=" + attempt(() => String.prototype.repeat.call(badThis, okCount)));
console.log("order3=" + order.join(","));

order.length = 0;
console.log("both-ok=" + attempt(() => String.prototype.repeat.call(okThis, okCount)));
console.log("order4=" + order.join(","));

// --- a NEGATIVE count still coerces the receiver first, so null this throws TypeError ---
console.log("null-this=" + attempt(() => String.prototype.repeat.call(null, 2)));
console.log("undefined-this=" + attempt(() => String.prototype.repeat.call(undefined, -1)));
console.log("number-this=" + attempt(() => String.prototype.repeat.call(12, 2)));
console.log("bool-this=" + attempt(() => String.prototype.repeat.call(true, 2)));
console.log("boxed-this=" + attempt(() => String.prototype.repeat.call(new String("ab"), 2)));

// --- astral receivers repeat by code unit, so the result stays well-formed ---
console.log("astral=" + attempt(() => "\u{1F600}".repeat(2).length));
console.log("astral-wf=" + "\u{1F600}".repeat(3).isWellFormed());
console.log("lone-wf=" + "\uD83D".repeat(2).isWellFormed());
console.log("lone-len=" + "\uD83D".repeat(2).length);

// --- the method's own shape ---
console.log("name=" + String.prototype.repeat.name);
console.log("length=" + String.prototype.repeat.length);
