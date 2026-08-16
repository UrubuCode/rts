// Pins the KEY SEQUENCE a proxy's get trap observes when the proxy is coerced:
// @@toPrimitive first every time, then valueOf/toString for a number-or-default
// hint and toString/valueOf for a string hint — and the trap can supply any of
// them for a target that has none.

const log: string[] = [];

function coercible(target: any): any {
  return new Proxy(target, {
    get(t, k, r) { log.push(String(k)); return Reflect.get(t, k, r); },
  });
}

function run(label: string, fn: (p: any) => string, target?: any): void {
  log.length = 0;
  const p = coercible(target === undefined ? { valueOf() { return 7; }, toString() { return "SEVEN"; } } : target);
  let out: string;
  try {
    out = fn(p);
  } catch (e: any) {
    out = "throw:" + e.constructor.name;
  }
  console.log(label + "=" + out + "|" + log.join(","));
}

run("String", (p) => String(p));
run("template", (p) => `${p}`);
run("concat_left", (p) => p + "");
run("plus_number", (p) => String(p + 1));
run("Number", (p) => String(Number(p)));
run("unary_plus", (p) => String(+p));
run("multiply", (p) => String(p * 2));
run("loose_eq_number", (p) => String(p == 7));
run("loose_eq_string", (p) => String(p == "SEVEN"));
run("relational", (p) => String(p < 8));
run("property_key", (p) => { const o: any = {}; o[p] = 1; return Object.keys(o).join("|"); });
run("array_join", (p) => [p].join(","));
run("string_concat_method", (p) => "x".concat(p as any));

// an object with only toString
run("only_toString", (p) => String(p * 1), { toString() { return "3"; } });
run("only_valueOf", (p) => String(p) + "", { valueOf() { return 4; } });
// a valueOf that answers an object is skipped in favour of toString
run("valueOf_object", (p) => String(p + ""), { valueOf() { return {}; }, toString() { return "FALLBACK"; } });
// neither usable is a TypeError
run("neither", (p) => String(p + ""), Object.create(null));

// @@toPrimitive wins and receives the hint
const hints: string[] = [];
const withSymbol: any = {
  [Symbol.toPrimitive](hint: string) { hints.push(hint); return hint === "string" ? "S" : 11; },
  valueOf() { return 99; },
  toString() { return "NEVER"; },
};
run("sym_string", (p) => String(p), withSymbol);
run("sym_number", (p) => String(+p), withSymbol);
run("sym_default", (p) => String(p + ""), withSymbol);
console.log("hints=" + hints.join(","));

// the get trap may inject @@toPrimitive for a target without one
log.length = 0;
const injected: any = new Proxy({} as any, {
  get(t, k, r) {
    log.push(String(k));
    if (k === Symbol.toPrimitive) return (hint: string) => "INJ_" + hint;
    return Reflect.get(t, k, r);
  },
});
console.log("injected_string=" + String(injected) + "|" + log.join(","));
log.length = 0;
console.log("injected_default=" + (injected + "") + "|" + log.join(","));

// a non-callable @@toPrimitive is a TypeError, but null and undefined mean
// "absent" and fall through to the ordinary methods
function coerce(label: string, sym: any): void {
  const t: any = { [Symbol.toPrimitive]: sym, valueOf() { return 1; }, toString() { return "T"; } };
  try {
    console.log(label + "=" + String(new Proxy(t, {}) + ""));
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}
coerce("sym_null", null);
coerce("sym_undefined", undefined);
coerce("sym_number", 5);
coerce("sym_object", {});

// a @@toPrimitive returning an object is a TypeError
try {
  console.log("sym_returns_object=" + String(new Proxy({ [Symbol.toPrimitive]() { return {}; } }, {}) + ""));
} catch (e: any) {
  console.log("sym_returns_object=throw:" + e.constructor.name);
}
