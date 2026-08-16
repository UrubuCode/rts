// ONE thing: NaN is the value that is not equal to itself, so every API that
// searches by "is it the same value" has to choose a relation — and they do not
// all choose the same one. indexOf uses strict equality and never finds it;
// includes, Set, Map and Object.is use SameValueZero (or SameValue) and do.

const nan: number = NaN;
const arr: number[] = [1, NaN, 3, NaN, 5];

// --- the four relations, on NaN alone ---
console.log("strict=" + (nan === nan));
console.log("loose=" + (nan == nan));
console.log("not_not_equal=" + (nan !== nan));
console.log("object_is=" + Object.is(nan, nan));
console.log("object_is_zero=" + Object.is(nan, 0));
console.log("self_compare_lt=" + (nan < nan) + " gt=" + (nan > nan) + " le=" + (nan <= nan) + " ge=" + (nan >= nan));
console.log("max_min=" + String(Math.max(1, NaN, 3)) + "," + String(Math.min(1, NaN, 3)));

// --- Array searching splits cleanly along the relation each one uses ---
console.log("indexOf=" + arr.indexOf(NaN));
console.log("lastIndexOf=" + arr.lastIndexOf(NaN));
console.log("includes=" + arr.includes(NaN));
console.log("find=" + String(arr.find((x) => Number.isNaN(x))));
console.log("findIndex=" + arr.findIndex((x) => Number.isNaN(x)));
console.log("findLastIndex=" + arr.findLastIndex((x) => Number.isNaN(x)));
console.log("some_strict=" + arr.some((x) => x === NaN));
console.log("some_isnan=" + arr.some((x) => Number.isNaN(x)));
console.log("every_not_nan=" + arr.every((x) => !Number.isNaN(x)));
console.log("filter_out=" + arr.filter((x) => !Number.isNaN(x)).join(","));
console.log("count=" + arr.filter(Number.isNaN).length);

// --- and on a typed array, which stores a NaN just as happily ---
const f64 = new Float64Array([1, NaN, 3]);
console.log("typed_indexOf=" + f64.indexOf(NaN));
console.log("typed_includes=" + f64.includes(NaN));
console.log("typed_join=" + f64.join(","));
console.log("typed_stays_nan=" + Number.isNaN(f64[1]));
const i32 = new Int32Array(1);
i32[0] = NaN;
console.log("int_array_nan_becomes=" + String(i32[0]));

// --- Set and Map treat all NaNs as one key ---
const set = new Set<number>([NaN, NaN, 0 / 0, Number("x"), Infinity - Infinity]);
console.log("set_size=" + set.size);
console.log("set_has=" + set.has(NaN));
console.log("set_values=" + Array.from(set).map((v) => String(v)).join(","));
const map = new Map<number, string>();
map.set(NaN, "first");
map.set(0 / 0, "second");
console.log("map_size=" + map.size + " get=" + String(map.get(NaN)));
console.log("map_keys=" + Array.from(map.keys()).map((k) => String(k)).join(","));
console.log("map_delete=" + map.delete(NaN) + " size_after=" + map.size);

// --- dedupe idioms: the Set one keeps NaN once, the filter/indexOf one drops
//     every NaN because indexOf never matches ---
const dupes: number[] = [1, NaN, 1, NaN, 2];
console.log("dedupe_set=" + Array.from(new Set(dupes)).map((v) => String(v)).join(","));
console.log("dedupe_indexOf=" + dupes.filter((v, i) => dupes.indexOf(v) === i).map((v) => String(v)).join(","));
console.log("dedupe_includes=" + dupes.reduce((acc: number[], v) => (acc.includes(v) ? acc : acc.concat([v])), []).map((v) => String(v)).join(","));

// --- as a property key it becomes the string "NaN", where identity returns ---
const obj: any = {};
obj[NaN] = "a";
obj[0 / 0] = "b";
console.log("object_keys=" + Object.keys(obj).join(",") + " value=" + obj[NaN] + " count=" + Object.keys(obj).length);
console.log("key_is_string=" + ("NaN" in obj) + "," + (obj["NaN"] === obj[NaN]));

// --- detection: only Number.isNaN and Object.is are safe ---
const probes: [string, any][] = [
  ["nan", NaN],
  ["string_nan", "NaN"],
  ["undefined", undefined],
  ["empty_string", ""],
  ["object", {}],
  ["zero", 0],
  ["null", null],
  ["array", []],
];
for (const p of probes) {
  console.log(
    p[0] +
      " | Number.isNaN:" + Number.isNaN(p[1]) +
      " | globalThis.isNaN:" + (globalThis as any).isNaN(p[1]) +
      " | Object.is:" + Object.is(p[1], NaN) +
      " | self_ne:" + (p[1] !== p[1])
  );
}

// --- sorting: the comparator returns NaN for every pair, and the result is
//     implementation-defined ORDER, so only the multiset is asserted ---
const sorted = [3, NaN, 1, NaN, 2].slice().sort((a, b) => a - b);
console.log("sort_length=" + sorted.length + " nan_count=" + sorted.filter(Number.isNaN).length);
console.log("default_sort=" + [3, NaN, 1, 2].slice().sort().map((v) => String(v)).join(","));
console.log("safe_sort=" + [3, NaN, 1, 2].filter((x) => !Number.isNaN(x)).sort((a, b) => a - b).join(","));

// --- JSON drops it to null, which is the only round trip that loses it ---
console.log("json=" + JSON.stringify({ a: NaN, b: [NaN] }));
console.log("json_roundtrip=" + String(JSON.parse(JSON.stringify({ a: NaN })).a));
console.log("string_form=" + String(NaN) + "," + `${NaN}` + "," + [NaN].join(""));
console.log("boolean_form=" + Boolean(NaN) + "," + (NaN ? "t" : "f"));
