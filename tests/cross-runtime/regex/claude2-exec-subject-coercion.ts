// Cross-runtime: exec/test coerce their argument with ToString, and they do it
// LATE — after lastIndex has been read — so an object whose toString has a side
// effect can observe the order, and a Symbol argument throws a TypeError rather
// than matching. The corpus always passes exec a string; this pins what happens
// when it is not one.

function attempt(f: () => any): string {
  try {
    const v = f();
    return v === null ? "null" : JSON.stringify(v);
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- primitives are stringified, so a number really can be matched ---
console.log("number=" + attempt(() => (/\d+/.exec(123 as any) as any)[0]));
console.log("float=" + attempt(() => (/\./.test(1.5 as any))));
console.log("exp=" + attempt(() => (/e\+/.test(1e21 as any))));
console.log("negzero=" + attempt(() => /^0$/.test(-0 as any)));
console.log("nan=" + attempt(() => /NaN/.test(NaN as any)));
console.log("infinity=" + attempt(() => /Infinity/.test(Infinity as any)));
console.log("true=" + attempt(() => /^true$/.test(true as any)));
console.log("null=" + attempt(() => /^null$/.test(null as any)));
console.log("undefined=" + attempt(() => /^undefined$/.test(undefined as any)));
console.log("no-arg=" + attempt(() => /^undefined$/.test()));
console.log("bigint=" + attempt(() => /^12$/.test(12n as any)));
console.log("symbol=" + attempt(() => /a/.test(Symbol("a") as any)));

// --- objects go through the ordinary OrdinaryToPrimitive(hint string) ---
console.log("array=" + attempt(() => (/a,b/.test(["a", "b"] as any))));
console.log("empty-array=" + attempt(() => /^$/.test([] as any)));
console.log("plain-object=" + attempt(() => /object Object/.test({} as any)));
console.log("tostring=" + attempt(() => /^hi$/.test({ toString: () => "hi" } as any)));
console.log("valueof-ignored=" + attempt(() => /^hi$/.test({ toString: () => "hi", valueOf: () => "bye" } as any)));
console.log("valueof-fallback=" + attempt(() => /^bye$/.test({ toString: null, valueOf: () => "bye" } as any)));
console.log("toPrimitive=" + attempt(() => /^tp$/.test({ [Symbol.toPrimitive]: () => "tp" } as any)));
console.log("toPrimitive-hint=" + attempt(() => {
  let hint = "";
  /x/.test({ [Symbol.toPrimitive]: (h: string) => { hint = h; return "x"; } } as any);
  return hint;
}));
console.log("throwing-tostring=" + attempt(() => /a/.test({ toString: () => { throw new RangeError("x"); } } as any)));
console.log("no-primitive=" + attempt(() => /a/.test(Object.create(null) as any)));

// --- the coercion is not cached: each call re-stringifies ---
let calls = 0;
const counted: any = { toString: () => { calls++; return "aa"; } };
/a/.test(counted);
/a/g.exec(counted);
console.log("calls=" + calls);

// --- exec reads lastIndex BEFORE coercing, so the side effect cannot move it ---
const re = /a/g;
re.lastIndex = 0;
const mover: any = { toString: () => { re.lastIndex = 1; return "aa"; } };
const r0: any = re.exec(mover);
console.log("order-index=" + r0.index + " after=" + re.lastIndex);

// --- the result's `input` is the COERCED string, not the original object ---
const res: any = /a/.exec({ toString: () => "xax" } as any);
console.log("input=" + JSON.stringify(res.input) + " type=" + typeof res.input);
console.log("input-num=" + JSON.stringify((/2/.exec(123 as any) as any).input));

// --- String.prototype methods coerce their RECEIVER the same way ---
console.log("match-on-number=" + attempt(() => String.prototype.match.call(123, /\d/) as any));
console.log("replace-on-number=" + attempt(() => String.prototype.replace.call(123, /2/, "-")));
console.log("search-on-bool=" + attempt(() => String.prototype.search.call(true, /r/)));
console.log("match-null-this=" + attempt(() => String.prototype.match.call(null, /a/)));
console.log("split-on-array=" + attempt(() => String.prototype.split.call(["a", "b"], /,/)));

// --- a string OBJECT is unwrapped, and matches identically ---
const boxed = new String("abc");
console.log("boxed-exec=" + attempt(() => (/b/.exec(boxed as any) as any)[0]));
console.log("boxed-input-type=" + typeof (/b/.exec(boxed as any) as any).input);
console.log("boxed-index=" + (/b/.exec(boxed as any) as any).index);

// --- test is exec !== null, so every coercion above behaves the same in both ---
console.log("test-vs-exec=" + (/\d/.test(5 as any) === (/\d/.exec(5 as any) !== null)));
