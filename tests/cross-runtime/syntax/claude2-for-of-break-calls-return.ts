// Cross-runtime: leaving a `for-of` early CLOSES the iterator — `break`,
// `return` and a labelled break all call the iterator's `return()` exactly
// once, while running to exhaustion never does.

const log: string[] = [];

function makeIterable(name: string, values: number[]): any {
  return {
    [Symbol.iterator]() {
      let i = 0;
      return {
        next() {
          log.push(name + ".next" + i);
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return(v: any) {
          log.push(name + ".return");
          return { value: v, done: true };
        },
      };
    },
  };
}

// 1) Running to the end never calls return().
log.length = 0;
const all: number[] = [];
for (const v of makeIterable("full", [1, 2])) all.push(v);
console.log("exhausted_values=" + all.join(","));
console.log("exhausted_log=" + log.join(" "));

// 2) `break` calls return() once, after the last next().
log.length = 0;
const upToTwo: number[] = [];
for (const v of makeIterable("brk", [1, 2, 3, 4])) {
  upToTwo.push(v);
  if (v === 2) break;
}
console.log("break_values=" + upToTwo.join(","));
console.log("break_log=" + log.join(" "));

// 3) `return` from the enclosing function closes it too.
function returnsEarly(): string {
  for (const v of makeIterable("ret", [1, 2, 3])) {
    if (v === 2) return "left-at-" + v;
  }
  return "finished";
}
log.length = 0;
console.log("return_result=" + returnsEarly());
console.log("return_log=" + log.join(" "));

// 4) A labelled break out of the OUTER loop closes both iterators, inner first.
log.length = 0;
outer: for (const a of makeIterable("A", [1, 2])) {
  for (const b of makeIterable("B", [10, 20])) {
    if (a === 1 && b === 20) break outer;
  }
}
console.log("labelled_break_log=" + log.join(" "));

// 5) `continue` does NOT close the iterator — the loop is still running.
log.length = 0;
const kept: number[] = [];
for (const v of makeIterable("cont", [1, 2, 3])) {
  if (v === 2) continue;
  kept.push(v);
}
console.log("continue_values=" + kept.join(","));
console.log("continue_log=" + log.join(" "));

// 6) An iterator without a `return` method is simply left alone; breaking is
//    still legal.
log.length = 0;
const noReturn: any = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        log.push("nr.next");
        return { value: i++, done: i > 5 };
      },
    };
  },
};
const nrSeen: number[] = [];
for (const v of noReturn) {
  nrSeen.push(v);
  if (v === 1) break;
}
console.log("no_return_values=" + nrSeen.join(","));
console.log("no_return_log=" + log.join(" "));

// 7) A generator observes the close as its `finally` running at the break.
const genLog: string[] = [];
function* counted(): Generator<number> {
  try {
    genLog.push("start");
    yield 1;
    genLog.push("between");
    yield 2;
    genLog.push("end");
  } finally {
    genLog.push("finally");
  }
}
const genSeen: number[] = [];
for (const v of counted()) {
  genSeen.push(v);
  if (v === 1) break;
}
console.log("gen_values=" + genSeen.join(","));
console.log("gen_log=" + genLog.join(","));

// 8) The same generator run to completion reaches `end` before `finally`.
genLog.length = 0;
const genAll: number[] = [];
for (const v of counted()) genAll.push(v);
console.log("gen_full_values=" + genAll.join(","));
console.log("gen_full_log=" + genLog.join(","));

// 9) A broken generator is done afterwards: resuming answers done.
const g = counted();
genLog.length = 0;
for (const v of g) {
  if (v === 1) break;
}
const after = g.next();
console.log("gen_after_break=" + after.done + "/" + String(after.value));

// 10) Built-in iterators have no `return`, so breaking out of a Map, a Set, an
//     array or a string is a no-op on the iterator itself.
function hasReturn(it: any): string {
  return typeof it.return;
}
console.log("array_iter_return=" + hasReturn([1, 2][Symbol.iterator]()));
console.log("map_iter_return=" + hasReturn(new Map([[1, 1]])[Symbol.iterator]()));
console.log("set_iter_return=" + hasReturn(new Set([1])[Symbol.iterator]()));
console.log("string_iter_return=" + hasReturn("ab"[Symbol.iterator]()));
console.log("gen_iter_return=" + hasReturn(counted()));

// 11) Breaking out of a Map/Set/string loop still stops at the right place.
const mapSeen: string[] = [];
for (const [k, v] of new Map<string, number>([["a", 1], ["b", 2], ["c", 3]])) {
  mapSeen.push(k + v);
  if (k === "b") break;
}
console.log("map_break=" + mapSeen.join(","));

const setSeen: number[] = [];
for (const v of new Set<number>([1, 2, 3])) {
  setSeen.push(v);
  if (v === 2) break;
}
console.log("set_break=" + setSeen.join(","));

const charsSeen: string[] = [];
for (const ch of "hello") {
  charsSeen.push(ch);
  if (ch === "l") break;
}
console.log("string_break=" + charsSeen.join(""));

// 12) A partially consumed array iterator kept in a variable resumes where the
//     broken loop left it.
const shared = [1, 2, 3, 4][Symbol.iterator]();
const firstPass: number[] = [];
for (const v of shared as any) {
  firstPass.push(v);
  if (v === 2) break;
}
const secondPass: number[] = [];
for (const v of shared as any) secondPass.push(v);
console.log("shared_first=" + firstPass.join(","));
console.log("shared_second=" + secondPass.join(","));

// 13) `return()` returning a non-object is tolerated when the loop leaves via
//     break (the spec ignores the value for a normal completion).
log.length = 0;
const oddReturn: any = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() { return { value: i++, done: false }; },
      return() { log.push("odd.return"); return { done: true }; },
    };
  },
};
for (const v of oddReturn) {
  if (v === 1) break;
}
console.log("odd_return_log=" + log.join(" "));

// 14) Nested for-of over the SAME iterable makes two independent iterators.
log.length = 0;
const twice = makeIterable("T", [1, 2]);
const pairsSeen: string[] = [];
for (const a of twice) {
  for (const b of twice) pairsSeen.push(a + "" + b);
}
console.log("independent=" + pairsSeen.join(","));
