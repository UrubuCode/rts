// Cross-runtime: for-of drives a hand-written Symbol.iterator returning a
// manual { next } object. Focus: the exact next()/done contract.

// 1) plain counter, done flips after 3 values
const counter = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        if (i <= 3) return { value: i * 10, done: false };
        return { value: undefined, done: true };
      }
    };
  }
};
const seen: number[] = [];
for (const v of counter) seen.push(v);
console.log("counter=" + seen.join(","));

// 2) iterable is re-iterable: a fresh iterator per for-of
const again: number[] = [];
for (const v of counter) again.push(v);
console.log("reiterate=" + again.join(","));

// 3) done:true on the FIRST next => zero iterations
const empty = {
  [Symbol.iterator]() {
    return { next() { return { value: 99, done: true }; } };
  }
};
let emptyCount = 0;
for (const _v of empty) emptyCount++;
console.log("emptyCount=" + emptyCount);

// 4) the value on a done:true result is IGNORED by for-of
const trailing = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        if (i === 1) return { value: "a", done: false };
        return { value: "IGNORED", done: true };
      }
    };
  }
};
const t: string[] = [];
for (const v of trailing) t.push(v);
console.log("trailing=" + t.join(",") + "|len=" + t.length);

// 5) done is coerced as truthy, not compared to === true
const truthyDone = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        // done: 0 (falsy) twice, then done: 1 (truthy)
        return { value: i, done: i > 2 ? 1 : 0 };
      }
    };
  }
};
const td: number[] = [];
for (const v of truthyDone as any) td.push(v);
console.log("truthyDone=" + td.join(","));

// 6) a MISSING done property is falsy => must keep going until explicit done
const missingDone = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        if (i <= 2) return { value: i }; // no done key at all
        return { done: true, value: undefined };
      }
    };
  }
};
const md: number[] = [];
for (const v of missingDone as any) md.push(v);
console.log("missingDone=" + md.join(","));

// 7) next() call count is exactly values+1 (one extra to observe done)
let calls = 0;
const counted = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        calls++;
        i++;
        return i <= 4 ? { value: i, done: false } : { value: undefined, done: true };
      }
    };
  }
};
let n = 0;
for (const _v of counted) n++;
console.log("values=" + n + "|nextCalls=" + calls);

// 8) the iterator object itself is returned by Symbol.iterator, not the iterable
const selfIter = {
  n: 0,
  next() {
    this.n++;
    return this.n <= 2 ? { value: "s" + this.n, done: false } : { value: undefined, done: true };
  },
  [Symbol.iterator]() {
    return this;
  }
};
const si: string[] = [];
for (const v of selfIter as any) si.push(v);
console.log("selfIter=" + si.join(","));
