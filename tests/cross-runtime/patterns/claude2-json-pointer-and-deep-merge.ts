// ONE thing: a whole program rather than one API — an RFC 6901 JSON Pointer
// resolver plus a deep merge with array strategies, over a fixed document. It
// exercises string escaping, recursion, property creation order and type
// dispatch together, which is where an engine bug shows up as a wrong answer
// rather than as a missing name.
const doc: any = {
  "": "empty-key",
  "a/b": "slash-key",
  "m~n": "tilde-key",
  " ": "space-key",
  users: [
    { id: 1, name: "ana", tags: ["x", "y"], meta: { active: true } },
    { id: 2, name: "bo", tags: [], meta: { active: false, note: null } },
  ],
  nested: { deep: { deeper: { value: 42 } } },
};

function unescape(tok: string): string {
  return tok.split("~1").join("/").split("~0").join("~");
}

function resolve(root: any, pointer: string): string {
  if (pointer === "") return "ROOT";
  if (pointer.charAt(0) !== "/") return "ERR:no-leading-slash";
  const toks = pointer.split("/").slice(1).map(unescape);
  let cur = root;
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    if (cur === null || typeof cur !== "object") return "ERR:not-container@" + i;
    if (Array.isArray(cur)) {
      if (t === "-") return "ERR:dash";
      if (!/^(0|[1-9][0-9]*)$/.test(t)) return "ERR:bad-index:" + t;
      const n = Number(t);
      if (n >= cur.length) return "ERR:oob:" + t;
      cur = cur[n];
    } else {
      if (!Object.prototype.hasOwnProperty.call(cur, t)) return "ERR:missing:" + JSON.stringify(t);
      cur = cur[t];
    }
  }
  return JSON.stringify(cur);
}

const pointers = [
  "", "/", "/a~1b", "/m~0n", "/ ", "/nested/deep/deeper/value",
  "/users/0/name", "/users/1/meta/note", "/users/0/tags/1", "/users/2",
  "/users/-", "/users/01", "/users/0/nope", "/nested/deep/deeper/value/more",
  "no-slash", "/users/0/tags",
];
for (const p of pointers) console.log("ptr " + JSON.stringify(p) + " => " + resolve(doc, p));

// --- deep merge, three array strategies, order of resulting keys pinned ---
function merge(a: any, b: any, arrays: string): any {
  if (Array.isArray(a) && Array.isArray(b)) {
    if (arrays === "replace") return b.slice();
    if (arrays === "concat") return a.concat(b);
    const out = a.slice();
    for (let i = 0; i < b.length; i++) out[i] = i < a.length ? merge(a[i], b[i], arrays) : b[i];
    return out;
  }
  if (a && b && typeof a === "object" && typeof b === "object" && !Array.isArray(a) && !Array.isArray(b)) {
    const out: any = {};
    for (const k of Object.keys(a)) out[k] = a[k];
    for (const k of Object.keys(b)) {
      out[k] = Object.prototype.hasOwnProperty.call(a, k) ? merge(a[k], b[k], arrays) : b[k];
    }
    return out;
  }
  return b;
}

const base = { z: 1, a: { p: [1, 2, 3], q: "keep" }, 2: "two", 10: "ten", 1: "one" };
const over = { a: { p: [9], r: "new" }, y: 2, 3: "three" };
for (const strat of ["replace", "concat", "index"]) {
  const m = merge(base, over, strat);
  console.log("merge:" + strat + " keys=" + Object.keys(m).join(",") + " json=" + JSON.stringify(m));
}

// Merging with null, undefined and a primitive on either side.
console.log("nullOver=" + JSON.stringify(merge({ a: 1 }, { a: null }, "replace")));
console.log("undefOver=" + JSON.stringify(merge({ a: 1 }, { a: undefined }, "replace")));
console.log("primOver=" + JSON.stringify(merge({ a: { b: 1 } }, { a: 5 }, "replace")));
console.log("objOverPrim=" + JSON.stringify(merge({ a: 5 }, { a: { b: 1 } }, "replace")));
console.log("arrOverObj=" + JSON.stringify(merge({ a: { b: 1 } }, { a: [1] }, "replace")));
