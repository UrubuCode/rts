// ONE thing: BigInt at the boundaries where JavaScript wants a string or an
// ordering. JSON.stringify throws on it by design, the default sort comparator
// compares its DECIMAL SPELLING, and a toJSON on BigInt.prototype is enough to
// make JSON.stringify succeed — which is the documented escape hatch.

function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- JSON.stringify refuses a BigInt wherever it appears ---
attempt("bare", () => JSON.stringify(1n));
attempt("in_object", () => JSON.stringify({ a: 1n }));
attempt("in_array", () => JSON.stringify([1n]));
attempt("nested", () => JSON.stringify({ a: { b: [1n] } }));
attempt("wrapper", () => JSON.stringify(Object(1n)));
attempt("as_key", () => JSON.stringify({ [1n as any]: 2 }));
attempt("zero", () => JSON.stringify(0n));

// --- a replacer that converts first is the ordinary workaround ---
attempt("replacer", () =>
  JSON.stringify({ a: 1n, b: 2 }, function (k: string, v: any) {
    return typeof v === "bigint" ? v.toString() : v;
  })
);
attempt("replacer_number", () =>
  JSON.stringify({ a: 5n }, function (k: string, v: any) {
    return typeof v === "bigint" ? Number(v) : v;
  })
);
attempt("replacer_sees_bigint", () => {
  const types: string[] = [];
  try {
    JSON.stringify({ a: 1n }, function (k: string, v: any) {
      types.push(typeof v);
      return typeof v === "bigint" ? "converted" : v;
    });
  } catch (e) {
    types.push("threw");
  }
  return types.join(",");
});

// --- the object's own toJSON is consulted before the type check ---
attempt("own_toJSON", () =>
  JSON.stringify({ a: { value: 1n, toJSON: function () { return "one"; } } })
);

// --- JSON.parse never produces a BigInt; a long integer loses precision ---
console.log("--- parse ---");
const parsed: any = JSON.parse("123456789012345678901234567890");
console.log("parsed_typeof=" + typeof parsed);
console.log("parsed_value=" + String(parsed));
const parsed2: any = JSON.parse('{"n":9007199254740993}');
console.log("parsed_lost=" + String(parsed2.n));
console.log("parsed_collides=" + (parsed2.n === 9007199254740992));
console.log("reviver_sees=" + typeof JSON.parse('{"n":1}', function (k: string, v: any) { return v; }).n);

// --- the default sort comparator compares decimal spellings ---
console.log("--- sort ---");
const nums: any[] = [10n, 9n, 1n, -1n, 2n, 100n, 0n];
console.log("default=" + nums.slice().sort().join(","));
console.log("numeric=" + nums.slice().sort(function (a: any, b: any) {
  return a < b ? -1 : a > b ? 1 : 0;
}).join(","));
console.log("mixed_default=" + ([10n, 9, 1n, 2] as any[]).slice().sort().join(","));
console.log("toSorted=" + (nums as any).toSorted().join(","));
attempt("comparator_returning_bigint", () =>
  nums.slice().sort(function (a: any, b: any) { return (a - b) as any; }).join(",")
);

// --- every string context uses the decimal spelling, radix 10 ---
console.log("--- stringification ---");
console.log("String=" + String(1234567890123456789012345n));
console.log("template=" + `${-42n}`);
console.log("concat=" + ("v=" + 42n));
console.log("join=" + [1n, 2n, 3n].join("-"));
console.log("array_toString=" + String([1n, 2n]));
console.log("Array_from_map=" + Array.from([1n, 2n], function (x: any) { return x * 2n; }).join(","));
console.log("toStringTag=" + (1n as any)[Symbol.toStringTag]);
console.log("keys_of_object=" + Object.keys({ [1n as any]: 0, [2n as any]: 0 }).join(","));

// --- installing toJSON on BigInt.prototype makes stringify succeed ---
console.log("--- toJSON escape hatch ---");
(BigInt.prototype as any).toJSON = function () {
  return this.toString();
};
attempt("with_toJSON_bare", () => JSON.stringify(1n));
attempt("with_toJSON_object", () => JSON.stringify({ a: 1n, b: [2n] }));
attempt("with_toJSON_wrapper", () => JSON.stringify(Object(3n)));
delete (BigInt.prototype as any).toJSON;
attempt("after_delete", () => JSON.stringify(1n));
console.log("toJSON_gone=" + ("toJSON" in BigInt.prototype));
