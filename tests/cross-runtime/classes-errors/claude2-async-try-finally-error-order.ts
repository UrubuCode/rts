// Cross-runtime: errors through async functions. A throw becomes a rejection
// with the SAME object, `finally` still overrides the completion, and an await
// inside a finally delays the propagation without changing which error wins.
const log: string[] = [];

const marker = new RangeError("marker");

async function throwsSync(): Promise<string> {
  throw marker;
}

async function throwsAfterAwait(): Promise<string> {
  await 0;
  throw marker;
}

async function finallyOverridesReturn(): Promise<string> {
  try {
    return "from-try";
  } finally {
    log.push("f1");
    return "from-finally";
  }
}

async function finallyOverridesThrow(): Promise<string> {
  try {
    throw new EvalError("inner");
  } finally {
    log.push("f2");
    return "rescued";
  }
}

async function finallyReplacesError(): Promise<string> {
  try {
    throw new EvalError("first");
  } finally {
    log.push("f3");
    throw new URIError("second");
  }
}

async function awaitInFinally(): Promise<string> {
  try {
    throw new EvalError("held");
  } finally {
    log.push("f4-before");
    await 0;
    log.push("f4-after");
  }
}

async function catchAndRethrow(): Promise<string> {
  try {
    await throwsAfterAwait();
    return "never";
  } catch (e: any) {
    log.push("caught:" + e.constructor.name + ":" + (e === marker));
    throw e;
  } finally {
    log.push("f5");
  }
}

async function settle(p: Promise<any>): Promise<string> {
  try {
    return "fulfilled:" + String(await p);
  } catch (e: any) {
    return "rejected:" + e.constructor.name;
  }
}

async function main(): Promise<void> {
  // The rejection carries the identical object.
  let same = "none";
  try {
    await throwsSync();
  } catch (e: any) {
    same = String(e === marker) + ":" + e.message;
  }
  console.log("identity=" + same);

  console.log("sync-throw=" + (await settle(throwsSync())));
  console.log("await-throw=" + (await settle(throwsAfterAwait())));
  console.log("finally-over-return=" + (await settle(finallyOverridesReturn())));
  console.log("finally-over-throw=" + (await settle(finallyOverridesThrow())));
  console.log("finally-replaces=" + (await settle(finallyReplacesError())));
  console.log("await-in-finally=" + (await settle(awaitInFinally())));
  console.log("rethrow=" + (await settle(catchAndRethrow())));
  console.log("log=" + log.join(">"));

  // An async function ALWAYS returns a promise, even when it throws before its
  // first await.
  const p = throwsSync();
  console.log("is-promise=" + (p instanceof Promise));
  console.log("settled=" + (await settle(p)));

  // Awaiting a non-promise thenable that throws from `then`.
  const badThenable: any = {
    then(): void {
      throw new SyntaxError("from-then");
    },
  };
  console.log("bad-thenable=" + (await settle(Promise.resolve().then(() => badThenable))));

  // A thenable that rejects.
  const rejecting: any = {
    then(_res: any, rej: any): void {
      rej(new URIError("thenable-reject"));
    },
  };
  console.log("rejecting-thenable=" + (await settle(Promise.resolve().then(() => rejecting))));

  // Promise.all rejects with the first error to arrive; allSettled reports all.
  const results = await Promise.allSettled([
    Promise.reject(new TypeError("a")),
    Promise.resolve("b"),
    throwsSync(),
  ]);
  console.log("allsettled=" + results.map((r: any) =>
    r.status === "fulfilled" ? "f:" + r.value : "r:" + r.reason.constructor.name).join("|"));
  console.log("all=" + (await settle(Promise.all([Promise.reject(new TypeError("x")), Promise.resolve(1)]))));

  // Promise.any collects every rejection into an AggregateError.
  let anyShape = "none";
  try {
    await Promise.any([Promise.reject(new TypeError("p")), Promise.reject(new RangeError("q"))]);
  } catch (e: any) {
    anyShape = e.constructor.name + ":" + e.errors.length + ":"
      + e.errors.map((x: any) => x.constructor.name).join(",")
      + ":" + (e instanceof AggregateError) + ":" + (e instanceof Error);
  }
  console.log("any=" + anyShape);

  // for await over an async iterable whose next rejects.
  const failing: any = {
    [Symbol.asyncIterator]() {
      let i = 0;
      return {
        next(): Promise<any> {
          i++;
          if (i === 2) {
            return Promise.reject(new EvalError("iter"));
          }
          return Promise.resolve({ value: "v" + i, done: i > 3 });
        },
        return(): Promise<any> {
          log.push("async-return");
          return Promise.resolve({ done: true });
        },
      };
    },
  };
  const seen: string[] = [];
  let iterOutcome = "none";
  try {
    for await (const v of failing) {
      seen.push(v);
    }
  } catch (e: any) {
    iterOutcome = e.constructor.name;
  }
  console.log("for-await-seen=" + seen.join(","));
  console.log("for-await-outcome=" + iterOutcome);
  console.log("for-await-log=" + log.join(">"));

  // An error in the BODY of a for-await closes the async iterator.
  const clean: any = {
    [Symbol.asyncIterator]() {
      let i = 0;
      return {
        next(): Promise<any> {
          i++;
          return Promise.resolve({ value: "c" + i, done: i > 3 });
        },
        return(): Promise<any> {
          log.push("clean-return");
          return Promise.resolve({ done: true });
        },
      };
    },
  };
  let bodyOutcome = "none";
  try {
    for await (const v of clean) {
      void v;
      throw new URIError("body");
    }
  } catch (e: any) {
    bodyOutcome = e.constructor.name;
  }
  console.log("body-outcome=" + bodyOutcome);
  console.log("final-log=" + log.join(">"));
}

main().then(() => {
  console.log("tail=reached");
});
console.log("sync-end=true");
