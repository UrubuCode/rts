// Cross-runtime: destructuring patterns in a `for-of` / `for-in` HEAD — one
// fresh binding per iteration for `let`/`const`, one shared binding for `var`,
// defaults re-evaluated every time, and a head that assigns to existing
// targets (including member expressions) instead of declaring.

const pairs: any[] = [[1, "a"], [2, "b"], [3, "c"]];

// 1) Array pattern in the head.
const seen: string[] = [];
for (const [n, s] of pairs) seen.push(n + s);
console.log("array_pattern=" + seen.join(","));

// 2) Object pattern with a default that only fires when the key is missing.
const defaultsRun: string[] = [];
function fb(which: string): string {
  defaultsRun.push(which);
  return "fb";
}
const objs: any[] = [{ x: 1, y: "Y" }, { x: 2 }, { x: 3, y: "Z" }];
const objSeen: string[] = [];
for (const { x, y = fb("y" + objs.length) } of objs) objSeen.push(x + ":" + y);
console.log("object_pattern=" + objSeen.join(","));
console.log("defaults_fired=" + defaultsRun.length);

// 3) Nested pattern with rest.
const nested: any[] = [[1, { tags: ["p", "q", "r"] }], [2, { tags: ["s"] }]];
const nestedSeen: string[] = [];
for (const [id, { tags: [first, ...others] }] of nested) {
  nestedSeen.push(id + "/" + first + "/" + others.join("-") + "(" + others.length + ")");
}
console.log("nested_pattern=" + nestedSeen.join(" "));

// 4) `const` in the head gives a NEW binding per iteration, so closures made in
//    the body do not share a cell.
const constClosures: Array<() => string> = [];
for (const [n, s] of pairs) constClosures.push(() => n + s);
console.log("const_closures=" + constClosures.map((f) => f()).join(","));

// 5) `var` in the head gives ONE function-scoped binding, so every closure
//    reads the last iteration's values.
function varHead(): string {
  const fns: Array<() => string> = [];
  for (var [vn, vs] of pairs) fns.push(() => vn + vs);
  return fns.map((f) => f()).join(",") + "|after=" + vn + vs;
}
console.log("var_head=" + varHead());

// 6) A head with no declaration assigns to bindings that already exist, and
//    they keep the LAST iteration's values afterwards.
let outerN = 0;
let outerS = "";
for ([outerN, outerS] of pairs) {
  /* body does not need the values */
}
console.log("assign_head_after=" + outerN + outerS);

// 7) The assignment head may target member expressions.
const sink: any = { last: null, key: null };
for ([sink.key, sink.last] of pairs) {
  /* each iteration overwrites */
}
console.log("member_targets=" + sink.key + sink.last);

// 8) An object pattern head over entries of a Map.
const m = new Map<string, number>([["one", 1], ["two", 2]]);
const mapSeen: string[] = [];
for (const [k, v] of m) mapSeen.push(k + "=" + v);
console.log("map_entries=" + mapSeen.join(","));

// 9) Destructuring a string yields characters, both directly and per entry.
const strSeen: string[] = [];
for (const [a, b] of ["xy", "zw"]) strSeen.push(a + "|" + b);
console.log("string_elements=" + strSeen.join(" "));

// 10) `for-in` gives string KEYS, so a pattern in its head destructures the key
//     itself.
const keyed: any = { ab: 1, cd: 2 };
const keySeen: string[] = [];
for (const [c0, c1] of Object.keys(keyed)) keySeen.push(c1 + c0);
console.log("keys_destructured=" + keySeen.join(","));

const inSeen: string[] = [];
for (const [f0] in keyed) inSeen.push(f0);
console.log("for_in_pattern=" + inSeen.join(","));

// 11) A default in the head is re-evaluated each iteration, so a counter in it
//     advances once per missing value.
let counter = 0;
function nextId(): number {
  counter += 1;
  return counter;
}
const withHoles: any[] = [{ id: 100 }, {}, {}, { id: 200 }];
const idSeen: number[] = [];
for (const { id = nextId() } of withHoles) idSeen.push(id);
console.log("default_counter=" + idSeen.join(",") + "|calls=" + counter);

// 12) Rest in the head collects into a fresh array each iteration.
const rests: any[] = [];
for (const [, ...tail] of pairs) rests.push(tail);
console.log("rest_arrays=" + rests.map((r) => "[" + r.join("") + "]").join("") +
  "|distinct=" + String(rests[0] !== rests[1]));

// 13) The pattern runs against `undefined` entries only through a default; an
//     absent property is fine, a missing OBJECT is not.
function missingObject(): string {
  try {
    for (const { z } of [undefined as any]) {
      return "reached:" + z;
    }
    return "loop-empty";
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("missing_object=" + missingObject());

// 14) Holes in the array pattern skip elements without consuming names.
const holeSeen: string[] = [];
for (const [, , third] of [[1, 2, 3], [4, 5, 6]]) holeSeen.push(String(third));
console.log("holes=" + holeSeen.join(","));

// 15) `let` in the head is per-iteration too, and mutating the binding inside
//     the body does not leak into the next iteration.
const mutSeen: string[] = [];
for (let [n] of pairs) {
  n = n * 10;
  mutSeen.push(String(n));
}
console.log("let_mutation=" + mutSeen.join(","));

// 16) Nested defaults inside a nested pattern.
const cfgs: any[] = [{ opts: { deep: 1 } }, { opts: {} }, {}];
const cfgSeen: string[] = [];
for (const { opts: { deep = -1 } = {} } of cfgs) cfgSeen.push(String(deep));
console.log("nested_defaults=" + cfgSeen.join(","));

// 17) A Set iterated with a pattern over its (value, value) entries.
const st = new Set<number>([7, 8]);
const setSeen: string[] = [];
for (const [a, b] of st.entries()) setSeen.push(a + "/" + b);
console.log("set_entries=" + setSeen.join(","));

// 18) The head's pattern is evaluated before the body — a throwing getter stops
//     the loop at that iteration.
const rows: any[] = [
  { get v(): number { return 1; } },
  { get v(): number { throw new TypeError("bad row"); } },
  { get v(): number { return 3; } },
];
function stopsOnGetter(): string {
  const out: string[] = [];
  try {
    for (const { v } of rows) out.push(String(v));
  } catch (e) {
    out.push("stopped:" + (e as any).constructor.name);
  }
  return out.join(",");
}
console.log("throwing_getter=" + stopsOnGetter());
