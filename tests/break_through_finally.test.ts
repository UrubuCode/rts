import { describe, test, expect } from "rts:test";

// A `break` or `continue` that leaves a `try` runs its `finally` on the way out --
// every `finally` it leaves, innermost first -- and a throw from one of them is not
// caught by the `catch` beside it.

function breakRuns(): string {
  const log: string[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      log.push("t" + i);
      if (i === 1) break;
    } finally {
      log.push("f" + i);
    }
  }
  return log.join();
}

function continueRuns(): string {
  const log: string[] = [];
  for (let i = 0; i < 3; i++) {
    try {
      if (i === 1) continue;
      log.push("t" + i);
    } finally {
      log.push("f" + i);
    }
  }
  return log.join();
}

function nestedInnermostFirst(): string {
  const log: string[] = [];
  outer: for (let i = 0; i < 2; i++) {
    try {
      for (let j = 0; j < 2; j++) {
        try {
          if (j === 1) break outer;
          log.push("b" + i + j);
        } finally {
          log.push("inner" + j);
        }
      }
    } finally {
      log.push("outer" + i);
    }
  }
  return log.join();
}

function fromTheCatch(): string {
  const log: string[] = [];
  while (true) {
    try {
      throw new Error("x");
    } catch (e) {
      log.push("c");
      break;
    } finally {
      log.push("f");
    }
  }
  return log.join();
}

function finallyThrowsPastItsCatch(): string {
  const log: string[] = [];
  try {
    for (;;) {
      try {
        break;
      } catch (e) {
        log.push("own catch");
      } finally {
        log.push("f");
        throw new Error("from finally");
      }
    }
  } catch (e) {
    log.push("outer:" + (e as Error).message);
  }
  return log.join();
}

function aLoopInsideTheTryIsNotLeft(): string {
  const log: string[] = [];
  try {
    for (let i = 0; i < 3; i++) {
      if (i === 1) break;
      log.push("i" + i);
    }
    log.push("after");
  } finally {
    log.push("f");
  }
  return log.join();
}

function finallyBreaksItself(): string {
  const log: string[] = [];
  outer: for (let i = 0; i < 3; i++) {
    for (let j = 0; j < 3; j++) {
      try {
        if (j === 1) continue outer;
        log.push("t" + i + j);
      } finally {
        log.push("f" + i + j);
        if (i === 1) break outer;
      }
    }
  }
  return log.join();
}

function finallyReturnsInstead(): string {
  const log: string[] = [];
  const run = (): string => {
    for (;;) {
      try {
        break;
      } finally {
        log.push("f");
        return "returned";
      }
    }
    return "fell";
  };
  return run() + "|" + log.join();
}

function iteratorClosedAndFinallyRun(): string {
  const log: string[] = [];
  const source = {
    [Symbol.iterator]() {
      let n = 0;
      return {
        next: () => ({ value: n++, done: n > 3 }),
        return: () => (log.push("close"), { value: undefined, done: true }),
      };
    },
  };
  outer: for (let k = 0; k < 1; k++) {
    try {
      for (const v of source) {
        if (v === 1) break outer;
        log.push("v" + v);
      }
    } finally {
      log.push("f");
    }
  }
  return log.join();
}

function labelledBreakClosesTheIterator(): string {
  const log: string[] = [];
  const source = {
    [Symbol.iterator]() {
      let n = 0;
      return {
        next: () => ({ value: n++, done: n > 3 }),
        return: () => (log.push("close"), { value: undefined, done: true }),
      };
    },
  };
  outer: for (let k = 0; k < 2; k++) {
    for (const v of source) {
      if (v === 1) continue outer;
      log.push(k + ":" + v);
    }
  }
  return log.join();
}

describe("break and continue through finally", () => {
  test("a labelled jump past a for-of closes its iterator", () =>
    expect(labelledBreakClosesTheIterator()).toBe("0:0,close,1:0,close"));
  test("a finally that breaks itself", () => expect(finallyBreaksItself()).toBe("t00,f00,f01,t10,f10"));
  test("a finally that returns replaces the break", () =>
    expect(finallyReturnsInstead()).toBe("returned|f"));
  test("an iterator closed, then the finally", () =>
    expect(iteratorClosedAndFinallyRun()).toBe("v0,close,f"));
  test("break", () => expect(breakRuns()).toBe("t0,f0,t1,f1"));
  test("continue", () => expect(continueRuns()).toBe("t0,f0,f1,t2,f2"));
  test("nested, innermost first", () =>
    expect(nestedInnermostFirst()).toBe("b00,inner0,inner1,outer0"));
  test("from the catch", () => expect(fromTheCatch()).toBe("c,f"));
  test("a finally that throws is not caught beside it", () =>
    expect(finallyThrowsPastItsCatch()).toBe("f,outer:from finally"));
  test("a loop inside the try", () => expect(aLoopInsideTheTryIsNotLeft()).toBe("i0,after,f"));
});
