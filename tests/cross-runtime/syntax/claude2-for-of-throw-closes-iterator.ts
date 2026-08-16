// Cross-runtime: an exception thrown from a `for-of` BODY closes the iterator
// on its way out, the body's exception is the one that escapes even if the
// close also throws, and a throwing `next()` is not closed at all.

const log: string[] = [];

function makeIterable(name: string, values: number[], opts?: any): any {
  const o = opts || {};
  return {
    [Symbol.iterator]() {
      let i = 0;
      return {
        next() {
          log.push(name + ".next");
          if (o.throwOnNext === i) throw new RangeError("next failed");
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return() {
          log.push(name + ".return");
          if (o.throwOnReturn) throw new TypeError("return failed");
          return { done: true };
        },
      };
    },
  };
}

// 1) A throw in the body calls return() before the exception leaves.
log.length = 0;
function bodyThrows(): string {
  try {
    for (const v of makeIterable("body", [1, 2, 3])) {
      if (v === 2) throw new RangeError("from body");
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("body_throws=" + bodyThrows());
console.log("body_throws_log=" + log.join(" "));

// 2) If return() ALSO throws, the body's exception is the one that survives.
log.length = 0;
function bothThrow(): string {
  try {
    for (const v of makeIterable("both", [1, 2, 3], { throwOnReturn: true })) {
      if (v === 1) throw new RangeError("body wins");
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("both_throw=" + bothThrow());
console.log("both_throw_log=" + log.join(" "));

// 3) A throwing return() DOES surface when the loop left by `break`, because
//    there is no exception to keep.
log.length = 0;
function breakWithThrowingReturn(): string {
  try {
    for (const v of makeIterable("brk", [1, 2, 3], { throwOnReturn: true })) {
      if (v === 1) break;
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("break_throwing_return=" + breakWithThrowingReturn());
console.log("break_throwing_return_log=" + log.join(" "));

// 4) A throwing next() is NOT followed by return() — the iterator never
//    reported a value to close over.
log.length = 0;
function nextThrows(): string {
  try {
    for (const v of makeIterable("nx", [1, 2, 3], { throwOnNext: 1 })) {
      log.push("body:" + v);
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("next_throws=" + nextThrows());
console.log("next_throws_log=" + log.join(" "));

// 5) A generator sees the body's throw as its `finally` running.
const genLog: string[] = [];
function* guarded(): Generator<number> {
  try {
    yield 1;
    yield 2;
  } finally {
    genLog.push("gen-finally");
  }
}
function genBodyThrows(): string {
  genLog.length = 0;
  try {
    for (const v of guarded()) {
      genLog.push("body" + v);
      if (v === 1) throw new RangeError("stop");
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("gen_body_throws=" + genBodyThrows());
console.log("gen_body_throws_log=" + genLog.join(","));

// 6) The generator is finished afterwards.
const g = guarded();
genLog.length = 0;
try {
  for (const v of g) {
    if (v === 1) throw new RangeError("halt");
  }
} catch (e) {
  genLog.push("outer:" + (e as any).constructor.name);
}
const resumed = g.next();
console.log("gen_after_throw=" + genLog.join(",") + "|done=" + resumed.done);

// 7) A generator whose finally itself throws: the body's exception still wins.
function* badFinally(): Generator<number> {
  try {
    yield 1;
  } finally {
    throw new TypeError("finally failed");
  }
}
function bodyVsFinally(): string {
  try {
    for (const v of badFinally()) {
      if (v === 1) throw new RangeError("body");
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("body_vs_finally=" + bodyVsFinally());

// 8) The same generator with a plain `break` lets the finally's exception out.
function breakVsFinally(): string {
  try {
    for (const v of badFinally()) {
      if (v === 1) break;
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("break_vs_finally=" + breakVsFinally());

// 9) A throw from the loop's own destructuring pattern closes the iterator too.
log.length = 0;
function patternThrows(): string {
  const rows: any = {
    [Symbol.iterator]() {
      let i = 0;
      const items = [
        { get v(): number { return 1; } },
        { get v(): number { throw new RangeError("bad getter"); } },
      ];
      return {
        next() {
          log.push("pat.next");
          return i < items.length ? { value: items[i++], done: false } : { done: true, value: undefined };
        },
        return() { log.push("pat.return"); return { done: true }; },
      };
    },
  };
  try {
    for (const { v } of rows) log.push("body:" + v);
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("pattern_throws=" + patternThrows());
console.log("pattern_throws_log=" + log.join(" "));

// 10) A `finally` in the loop body runs before the iterator is closed.
log.length = 0;
function bodyFinallyOrder(): string {
  try {
    for (const v of makeIterable("ord", [1, 2])) {
      try {
        throw new RangeError("x");
      } finally {
        log.push("body-finally");
      }
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("body_finally_order=" + bodyFinallyOrder());
console.log("body_finally_order_log=" + log.join(" "));

// 11) An iterable whose Symbol.iterator is not callable fails before any close.
function badIterable(): string {
  try {
    for (const v of { [Symbol.iterator]: 5 } as any) {
      return "reached:" + v;
    }
    return "loop-empty";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("bad_iterable=" + badIterable());

// 12) A non-iterable value fails with a TypeError, and the loop body never ran.
function notIterable(): string {
  let entered = false;
  try {
    for (const v of 42 as any) entered = true;
    return "loop-empty";
  } catch (e) {
    return "caught:" + (e as any).constructor.name + "|entered=" + entered;
  }
}
console.log("not_iterable=" + notIterable());

// 13) Nested loops: an inner throw closes the inner iterator, then the outer.
log.length = 0;
function nestedClose(): string {
  try {
    for (const a of makeIterable("O", [1, 2])) {
      for (const b of makeIterable("I", [10, 20])) {
        throw new RangeError("deep");
      }
    }
    return "no-throw";
  } catch (e) {
    return "caught:" + (e as any).constructor.name;
  }
}
console.log("nested_close=" + nestedClose());
console.log("nested_close_log=" + log.join(" "));
