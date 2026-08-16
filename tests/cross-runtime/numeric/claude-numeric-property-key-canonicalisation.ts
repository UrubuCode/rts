// ONE thing: a numeric property key becomes an array INDEX only if its string
// form is canonical. 1, "1", 1.0 and 1n are one key; "01", "1.0" and 1e21 are
// four different ordinary string keys that never touch `length`.

const a: any = ["a", "b", "c"];

// --- the same slot reached five ways ---
console.log("num=" + a[1]);
console.log("str=" + a["1"]);
console.log("float=" + a[1.0]);
console.log("bigint=" + a[1n]);
console.log("computed=" + a[0 + 1]);
console.log("all_same=" + (a[1] === a["1"] && a["1"] === a[1n]));

// --- the non-canonical spellings are ordinary keys ---
a["01"] = "zero-one";
a["1.0"] = "one-point-zero";
a[" 1"] = "space-one";
a["+1"] = "plus-one";
a["1e0"] = "exp-one";
console.log("len_after_fakes=" + a.length);
console.log("fake_01=" + a["01"]);
console.log("fake_1p0=" + a["1.0"]);
console.log("real_1_untouched=" + a[1]);
console.log("keys=" + Object.keys(a).join(","));

// --- negative zero stringifies to "0", so a[-0] is a[0] ---
const z: any = ["first"];
console.log("negzero=" + z[-0]);
z[-0] = "replaced";
console.log("negzero_write=" + z[0]);
console.log("negzero_len=" + z.length);

// The 2^32-2 array-index boundary lives in its own fixture
// (array/claude-array-index-upper-boundary.ts) because it is the one case that
// forces sparse storage: an engine that materialises the slots hangs on it, and
// a hang here would hide everything below.

// --- large and fractional numbers stringify to non-index keys ---
const o: any = {};
const numericKeys: [string, any][] = [
  ["int", 1],
  ["negint", -1],
  ["frac", 1.5],
  ["negzero", -0],
  ["nan", NaN],
  ["inf", Infinity],
  ["neginf", -Infinity],
  ["e21", 1e21],
  ["e_neg7", 1e-7],
  ["maxsafe", 9007199254740991],
  ["bigint", 10n],
];
for (const k of numericKeys) {
  o[k[1]] = k[0];
}
console.log("obj_keys=" + Object.keys(o).join("|"));
console.log("obj_read_negzero=" + o[0]);
console.log("obj_read_bigint_as_str=" + o["10"]);
console.log("obj_read_e21=" + o["1e+21"]);
console.log("obj_read_inf=" + o["Infinity"]);

// --- integer keys sort ascending and come before every string key ---
const mixed: any = {};
mixed["b"] = 1;
mixed[2] = 2;
mixed["a"] = 3;
mixed[10] = 4;
mixed[1] = 5;
mixed["01"] = 6;
mixed[0] = 7;
console.log("mixed_keys=" + Object.keys(mixed).join(","));
console.log("mixed_ownKeys=" + Reflect.ownKeys(mixed).map(String).join(","));
console.log("mixed_forin=" + (function () {
  const acc: string[] = [];
  for (const k in mixed) acc.push(k);
  return acc.join(",");
})());
console.log("mixed_json=" + JSON.stringify(mixed));

// --- the same rule governs delete and defineProperty ---
const d: any = [10, 20, 30];
delete d[1.0];
console.log("after_delete_len=" + d.length);
console.log("after_delete_read=" + String(d[1]));
console.log("after_delete_has=" + (1 in d));
Object.defineProperty(d, 3n as any, { value: 40, enumerable: true, writable: true, configurable: true });
console.log("defineProperty_bigint_len=" + d.length);
console.log("defineProperty_bigint_read=" + d[3]);
