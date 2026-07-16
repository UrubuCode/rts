// Cross-runtime: array entries()/keys()/values() consumed by for-of WITH pair
// destructuring in the loop head. Focus: the destructured-pair binding form.

const arr = ["a", "b", "c"];

// 1) the canonical form: for (const [i, v] of arr.entries())
const out1: string[] = [];
for (const [i, v] of arr.entries()) out1.push(i + ":" + v);
console.log("entries=" + out1.join(","));

// 2) index-only via a hole in the pattern
const out2: number[] = [];
for (const [i] of arr.entries()) out2.push(i);
console.log("idxOnly=" + out2.join(","));

// 3) value-only via a leading hole
const out3: string[] = [];
for (const [, v] of arr.entries()) out3.push(v);
console.log("valOnly=" + out3.join(","));

// 4) the pair is a real 2-element Array
const shapes: string[] = [];
for (const p of arr.entries()) shapes.push(String(Array.isArray(p)) + p.length);
console.log("pairShape=" + shapes.join(","));

// 5) a default in the pattern applies to an out-of-range slot
const out5: string[] = [];
for (const [i, v, extra = "DEF"] of arr.entries() as any) out5.push(i + v + extra);
console.log("withDefault=" + out5.join(","));

// 6) rest in the pattern collects the remainder of the pair
const out6: string[] = [];
for (const [i, ...restPair] of arr.entries()) out6.push(i + "->" + restPair.join("+"));
console.log("restPair=" + out6.join(","));

// 7) keys() and values() in for-of
const k: number[] = [];
for (const key of arr.keys()) k.push(key);
console.log("keys=" + k.join(","));
const vv: string[] = [];
for (const val of arr.values()) vv.push(val);
console.log("values=" + vv.join(","));

// 8) values() matches the default iterator's output
const dflt: string[] = [];
for (const val of arr) dflt.push(val);
console.log("valuesMatchDefault=" + (dflt.join(",") === vv.join(",")));

// 9) `let` in the head rebinds per iteration
const out9: string[] = [];
for (let [i, v] of arr.entries()) {
  i = i * 10;
  v = v.toUpperCase();
  out9.push(i + v);
}
console.log("letRebind=" + out9.join(","));

// 10) break stops early and leaves the rest unread
const out10: string[] = [];
for (const [i, v] of arr.entries()) {
  if (i === 1) break;
  out10.push(v);
}
console.log("break=" + out10.join(",") + "|len=" + out10.length);

// 11) continue skips
const out11: string[] = [];
for (const [i, v] of arr.entries()) {
  if (i === 1) continue;
  out11.push(v);
}
console.log("continue=" + out11.join(","));

// 12) a SPARSE array: entries() visits holes as undefined
const sparse = ["x", , "z"];
const out12: string[] = [];
for (const [i, v] of sparse.entries()) out12.push(i + ":" + String(v));
console.log("sparse=" + out12.join(","));

// 13) an EMPTY array: zero iterations
let emptyN = 0;
for (const [_i, _v] of ([] as string[]).entries()) emptyN++;
console.log("empty=" + emptyN);

// 14) entries() is live: push during iteration is seen
const live = ["p"];
const out14: string[] = [];
for (const [i, v] of live.entries()) {
  out14.push(i + v);
  if (i === 0) live.push("q");
  if (i > 3) break;
}
console.log("live=" + out14.join(","));

// 15) nested for-of over two entries() iterators
const pairsOut: string[] = [];
for (const [i, v] of ["m", "n"].entries()) {
  for (const [j, w] of ["1", "2"].entries()) pairsOut.push(i + v + j + w);
}
console.log("nested=" + pairsOut.join(","));

// 16) destructuring the pair OUTSIDE the head, from a manual next()
const eIt = arr.entries();
const [ei, ev] = eIt.next().value as [number, string];
console.log("manualPair=" + ei + ev);
