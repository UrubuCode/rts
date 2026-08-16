// Pins WHICH string keys count as "array index" for own-key ordering: the
// canonical numeric strings in [0, 2^32-2] sort ascending before every other
// string key. Existing order fixtures stop at small integers and never touch
// the 4294967294/4294967295 boundary or non-canonical forms like "01"/"+1".

const o: any = {};
o["zzz"] = "s0";
o["4294967295"] = "not-index";
o["4294967294"] = "last-index";
o["10"] = "ten";
o["-1"] = "neg";
o["01"] = "leading-zero";
o["1"] = "one";
o["1e2"] = "exp";
o["100"] = "hundred";
o["-0"] = "neg-zero";
o["0"] = "zero";
o["+1"] = "plus-one";
o["1.0"] = "dot-zero";
o[" 1"] = "space-one";
o["0x1"] = "hex";
o["NaN"] = "nan";
o["Infinity"] = "inf";
o["9007199254740993"] = "big";

console.log("keys=" + Object.keys(o).join("|"));
console.log("ownnames=" + Object.getOwnPropertyNames(o).join("|"));
console.log("reflect=" + Reflect.ownKeys(o).join("|"));
console.log("values=" + Object.values(o).join("|"));

const forin: string[] = [];
for (const k in o) forin.push(k);
console.log("forin=" + forin.join("|"));
console.log("forin_eq_keys=" + (forin.join("|") === Object.keys(o).join("|")));

const json = JSON.stringify(o);
console.log("json_first=" + json.slice(0, 40));
console.log("assign=" + Object.keys(Object.assign({}, o)).join("|"));
console.log("spread=" + Object.keys({ ...o }).join("|"));
console.log("entries0=" + Object.entries(o)[0].join("="));

// numeric keys inserted out of order still come out ascending
const n: any = {};
n[9] = "n9";
n[3] = "n3";
n[11] = "n11";
n[2] = "n2";
console.log("numeric=" + Object.keys(n).join("|"));

// The same boundary ON AN ARRAY lives in array/claude-array-index-upper-boundary.ts.
// It is kept apart because writing at 2^32-2 forces sparse storage: an engine
// that materialises the slots hangs, and a hang here would hide the object-key
// question this file is actually about.

// key coercion: the number 1 and the string "1" are the same own key
const c: any = {};
c[1] = "num";
c["1"] = "str";
console.log("collide=" + Object.keys(c).length + ":" + c[1]);

// a float-valued key is a plain string key on an array as well
const f: any = [];
f["1.5"] = "half";
f[2] = "two";
console.log("float_len=" + f.length + ",keys=" + Object.keys(f).join("|"));

// getOwnPropertyNames on an array puts length last, after every index
const g: any = [1, 2];
g.tail = "T";
console.log("gopn_arr=" + Object.getOwnPropertyNames(g).join("|"));

// -0 as a computed key canonicalises to "0"
const z: any = {};
z[-0] = "minus-zero";
console.log("minuszero_key=" + Object.keys(z).join("|"));
console.log("minuszero_read=" + z["0"]);
