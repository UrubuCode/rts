/**
 * Level-1 Silent Parallelism — purity pass demo.
 *
 * The RTS compiler analyses pure `for...of` loops at compile time and
 * rewrites them to `parallel.for_each` backed by a Rayon thread pool.
 * No annotation required: if the body only calls pure namespace members
 * and only touches the loop variable + inner decls, the rewrite happens
 * automatically ("silent parallelism").
 */

import { math, parallel, collections, gc, io } from "rts";

// ─────────────────────────────────────────────────────────────────────────────
// 1) Silent rewrite: pure for...of → parallel.for_each
//    Body only calls math.sin / math.cos — both pure. Compiler rewrites this
//    loop to a Rayon parallel iteration automatically.
// ─────────────────────────────────────────────────────────────────────────────
const angles: number[] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

for (const x of angles) {
  const s = math.sin(x as number);
  const c = math.cos(s);
  // Results discarded — pure side-effect-free computation,
  // each iteration fully independent.
}
io.print("silent parallel loop done");

// ─────────────────────────────────────────────────────────────────────────────
// 2) Multiple independent pure loops — each gets its own Rayon job.
// ─────────────────────────────────────────────────────────────────────────────
const vals: number[] = [1, 4, 9, 16, 25, 36, 49, 64];

for (const v of vals) {
  const r = math.sqrt(v as number);
}

for (const v of vals) {
  const l = math.log2(v as number);
}
io.print("multiple silent loops done");

// ─────────────────────────────────────────────────────────────────────────────
// 3) Explicit parallel.map — square each element.
// ─────────────────────────────────────────────────────────────────────────────
function squareIt(x: number): number {
  return x * x;
}

const sfp = squareIt as unknown as number;
const src = [1, 2, 3, 4, 5, 6, 7, 8];
const squared = parallel.map(src, sfp);

const h0 = gc.string_from_i64(collections.vec_get(squared, 0));
io.print(h0); gc.string_free(h0);   // 1

const h7 = gc.string_from_i64(collections.vec_get(squared, 7));
io.print(h7); gc.string_free(h7);   // 64

// ─────────────────────────────────────────────────────────────────────────────
// 4) Explicit parallel.reduce — sum of [1..8] = 36
// ─────────────────────────────────────────────────────────────────────────────
function addUp(acc: number, x: number): number {
  return acc + x;
}

const afp = addUp as unknown as number;
const total = parallel.reduce(src, 0, afp);

const ht = gc.string_from_i64(total);
io.print(ht); gc.string_free(ht);   // 36

// ─────────────────────────────────────────────────────────────────────────────
// 5) Thread count
// ─────────────────────────────────────────────────────────────────────────────
const nt = parallel.num_threads();
const hnt = gc.string_from_i64(nt);
io.print(hnt); gc.string_free(hnt);
io.print("done");
