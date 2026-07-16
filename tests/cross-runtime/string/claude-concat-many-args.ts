// Cross-runtime: String.prototype.concat argument handling — arity from 0 to
// many, and ToString coercion of every non-string argument (concat coerces each
// argument with ToString, exactly like the `+` operator does for objects).

const base = "x";

// --- arity: none, one, many ---
console.log("zero=[" + base.concat() + "]");
console.log("one=" + base.concat("a"));
console.log("two=" + base.concat("a", "b"));
console.log("five=" + base.concat("a", "b", "c", "d", "e"));
console.log("ten=" + "".concat("0", "1", "2", "3", "4", "5", "6", "7", "8", "9"));

// --- many arguments via apply + a generated list ---
const many: string[] = [];
for (let i = 0; i < 64; i++) many.push(String(i % 10));
const big = String.prototype.concat.apply("", many);
console.log("apply64-len=" + big.length);
console.log("apply64-head=" + big.slice(0, 12));
console.log("apply64-tail=" + big.slice(-4));

// --- spread of an array of strings ---
console.log("spread=" + "".concat(...["p", "q", "r"]));
console.log("spread-empty-len=" + "".concat(...[]).length);

// --- empty-string arguments contribute nothing ---
console.log("empties=[" + "".concat("", "", "") + "]");
console.log("empties-len=" + "".concat("", "", "").length);
console.log("interleaved=" + "a".concat("", "b", "", "c"));

// --- numbers are coerced with ToString ---
console.log("int=" + "n=".concat(42 as any));
console.log("zero-num=" + "n=".concat(0 as any));
console.log("neg-zero=" + "n=".concat(-0 as any));
console.log("float=" + "n=".concat(1.5 as any));
console.log("exp=" + "n=".concat(1e21 as any));
console.log("tiny=" + "n=".concat(1e-7 as any));
console.log("nan=" + "n=".concat(NaN as any));
console.log("inf=" + "n=".concat(Infinity as any, ",", -Infinity as any));

// --- booleans, null, undefined ---
console.log("bools=" + "b=".concat(true as any, ",", false as any));
console.log("null=" + "v=".concat(null as any));
console.log("undefined=" + "v=".concat(undefined as any));
console.log("null-undef=" + "".concat(null as any, "|", undefined as any));

// --- arrays join with commas; nested arrays flatten via ToString ---
console.log("arr=" + "a=".concat([1, 2, 3] as any));
console.log("arr-empty=[" + "".concat([] as any) + "]");
console.log("arr-nested=" + "".concat([[1], [2, 3]] as any));
console.log("arr-holes=" + "".concat([null, undefined, 1] as any));

// --- plain objects stringify to [object Object] ---
console.log("obj=" + "".concat({} as any));

// --- an object with a custom toString is honored ---
const custom: any = { toString: () => "CUSTOM" };
console.log("custom-tostring=" + "".concat(custom));

// --- valueOf is NOT preferred for a string hint... concat uses ToString ---
const both: any = { toString: () => "STR", valueOf: () => 99 };
console.log("tostring-wins=" + "".concat(both));

// --- mixed argument types in one call ---
console.log("mixed=" + "".concat("a", 1 as any, true as any, null as any, undefined as any, [2] as any));

// --- concat never mutates the receiver ---
const src = "keep";
src.concat("more");
console.log("no-mutate=" + src);
console.log("no-mutate-len=" + src.length);

// --- result equals the equivalent + expression ---
console.log("eq-plus=" + ("a".concat("b", "c") === "a" + "b" + "c"));

// --- concat on a non-string `this` via call ---
console.log("call-num=" + String.prototype.concat.call(5 as any, "!"));
console.log("call-bool=" + String.prototype.concat.call(true as any, "!"));

// --- surrogate pairs survive concatenation of their halves ---
const hi = String.fromCharCode(0xd83d);
const lo = String.fromCharCode(0xde00);
console.log("halves-len=" + hi.concat(lo).length);
console.log("halves-cp=" + hi.concat(lo).codePointAt(0).toString(16));
