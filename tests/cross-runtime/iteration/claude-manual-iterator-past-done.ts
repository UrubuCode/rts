// Cross-runtime: driving built-in iterators BY HAND with .next(), including
// what happens after done:true. Focus: the exhausted-iterator contract.

// 1) array iterator: next() to done, then twice more
const it = [1, 2][Symbol.iterator]();
console.log("a1=" + JSON.stringify(it.next()));
console.log("a2=" + JSON.stringify(it.next()));
console.log("a3=" + JSON.stringify(it.next()));
console.log("a4=" + JSON.stringify(it.next()));
console.log("a5=" + JSON.stringify(it.next()));

// 2) an exhausted iterator stays done forever (idempotent)
const ex = [9][Symbol.iterator]();
ex.next();
ex.next();
const after1 = ex.next();
const after2 = ex.next();
console.log("stableDone=" + (after1.done === after2.done) + "|" + after1.done);
console.log("stableValue=" + (after1.value === undefined) + "," + (after2.value === undefined));

// 3) each next() returns a FRESH result object (not a reused one)
const fresh = [1, 2][Symbol.iterator]();
const r1 = fresh.next();
const r2 = fresh.next();
console.log("freshResults=" + (r1 !== r2));

// 4) a result object is a plain object with exactly value+done
const shapeIt = [5][Symbol.iterator]();
const shaped = shapeIt.next();
console.log("keys=" + Object.keys(shaped).sort().join(","));

// 5) empty array: first next() is already done
const emptyIt = [][Symbol.iterator]();
console.log("emptyFirst=" + JSON.stringify(emptyIt.next()));

// 6) two independent iterators over the SAME array do not share position
const shared = [1, 2, 3];
const i1 = shared[Symbol.iterator]();
const i2 = shared[Symbol.iterator]();
i1.next();
i1.next();
console.log("independent=" + i1.next().value + "," + i2.next().value);

// 7) mutation AFTER creating the iterator is visible (live, index-based)
const live = [1, 2];
const liveIt = live[Symbol.iterator]();
liveIt.next();
live.push(3);
console.log("live=" + liveIt.next().value + "," + liveIt.next().value + "|done=" + liveIt.next().done);

// 8) shrinking the array mid-iteration ends it early
const shrink = [1, 2, 3, 4];
const shIt = shrink[Symbol.iterator]();
shIt.next();
shrink.length = 1;
console.log("shrunk=" + JSON.stringify(shIt.next()));

// 9) a MANUAL iterator: calling next() after it reported done
let manualCalls = 0;
const manual = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        manualCalls++;
        i++;
        return i <= 1 ? { value: "m", done: false } : { value: "post", done: true };
      }
    };
  }
};
const mIt = (manual as any)[Symbol.iterator]();
console.log("m1=" + JSON.stringify(mIt.next()));
console.log("m2=" + JSON.stringify(mIt.next()));
console.log("m3=" + JSON.stringify(mIt.next()));
console.log("manualCalls=" + manualCalls);

// 10) a half-drained iterator finished by for-of resumes where it left off
const half = [1, 2, 3, 4][Symbol.iterator]();
half.next();
const rest: number[] = [];
for (const v of half as any) rest.push(v);
console.log("resume=" + rest.join(","));

// 11) spread of a half-drained iterator takes only the remainder
const half2 = [1, 2, 3, 4][Symbol.iterator]();
half2.next();
half2.next();
console.log("spreadRest=" + [...(half2 as any)].join(","));

// 12) string iterator by hand past done
const sIt = "ab"[Symbol.iterator]();
console.log("s1=" + sIt.next().value);
console.log("s2=" + sIt.next().value);
console.log("s3=" + String(sIt.next().value) + "|done=" + sIt.next().done);

// 13) Set iterator by hand past done
const setIt = new Set([1])[Symbol.iterator]();
console.log("set1=" + JSON.stringify(setIt.next()));
console.log("set2=" + JSON.stringify(setIt.next()));
console.log("set3=" + JSON.stringify(setIt.next()));
