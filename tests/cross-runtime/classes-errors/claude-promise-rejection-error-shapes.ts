// Cross-runtime: what each Promise combinator does with a rejection — the first
// reason for all/race, the reason list for allSettled, an AggregateError for
// any — and the order the handlers observe. Only constructors and own messages
// are printed.
const log: string[] = [];

function tag(e: any): string {
  return e && e.constructor ? e.constructor.name : String(e);
}

const step1 = Promise.all([
  Promise.resolve(1),
  Promise.reject(new TypeError("first")),
  Promise.reject(new RangeError("second")),
]).then(
  () => {
    log.push("all-resolved");
  },
  (e: any) => {
    log.push("all-rejected=" + tag(e) + ":" + e.message);
  },
);

const step2 = step1.then(() =>
  Promise.race([Promise.reject(new EvalError("race")), Promise.resolve("later")]).then(
    () => {
      log.push("race-resolved");
    },
    (e: any) => {
      log.push("race-rejected=" + tag(e));
    },
  ),
);

const step3 = step2.then(() =>
  Promise.allSettled([Promise.resolve("v"), Promise.reject(new URIError("s"))]).then((rs: any[]) => {
    log.push("settled-len=" + rs.length);
    log.push("settled-0=" + rs[0].status + ":" + rs[0].value);
    log.push("settled-1=" + rs[1].status + ":" + tag(rs[1].reason));
    log.push("settled-0-keys=" + Object.keys(rs[0]).join(","));
    log.push("settled-1-keys=" + Object.keys(rs[1]).join(","));
  }),
);

const step4 = step3.then(() =>
  Promise.any([Promise.reject(new TypeError("a")), Promise.resolve("won"), Promise.reject(new RangeError("b"))]).then(
    (v: any) => {
      log.push("any-resolved=" + v);
    },
    () => {
      log.push("any-rejected");
    },
  ),
);

const step5 = step4.then(() =>
  Promise.any([Promise.reject(new TypeError("a")), Promise.reject(new RangeError("b"))]).then(
    () => {
      log.push("any2-resolved");
    },
    (e: any) => {
      log.push("any2=" + tag(e) + ":" + e.errors.length + ":" + tag(e.errors[0]) + ":" + tag(e.errors[1]));
    },
  ),
);

// A throw inside a then handler becomes a rejection of the derived promise.
const step6 = step5.then(() =>
  Promise.resolve(1)
    .then(() => {
      throw new SyntaxError("in-handler");
    })
    .catch((e: any) => {
      log.push("handler-throw=" + tag(e));
      return "recovered";
    })
    .then((v: any) => {
      log.push("recovered=" + v);
    }),
);

// A throw inside the executor rejects the promise being constructed.
const step7 = step6.then(
  () =>
    new Promise(() => {
      throw new EvalError("executor");
    }).then(
      () => {
        log.push("executor-resolved");
      },
      (e: any) => {
        log.push("executor=" + tag(e));
      },
    ),
);

// After resolve(), a later throw in the executor is swallowed.
const step8 = step7.then(
  () =>
    new Promise<string>((resolve) => {
      resolve("done");
      throw new EvalError("late");
    }).then(
      (v: any) => {
        log.push("late-throw-resolved=" + v);
      },
      (e: any) => {
        log.push("late-throw-rejected=" + tag(e));
      },
    ),
);

// finally passes the rejection through, and a throw inside it replaces it.
const step9 = step8.then(() =>
  Promise.reject(new RangeError("pass"))
    .finally(() => {
      log.push("finally-ran");
    })
    .catch((e: any) => {
      log.push("after-finally=" + tag(e));
    }),
);

const step10 = step9.then(() =>
  Promise.reject(new RangeError("replaced"))
    .finally(() => {
      throw new URIError("from-finally");
    })
    .catch((e: any) => {
      log.push("finally-throw=" + tag(e));
    }),
);

// Rejecting with a non-Error keeps the value verbatim.
const step11 = step10.then(() =>
  Promise.reject("plain-string").catch((e: any) => {
    log.push("non-error=" + typeof e + ":" + String(e));
  }),
);

// A thenable that throws from its then() rejects the adopting promise.
const step12 = step11.then(() =>
  Promise.resolve({
    then(): void {
      throw new TypeError("thenable");
    },
  } as any).catch((e: any) => {
    log.push("thenable=" + tag(e));
  }),
);

step12.then(() => {
  for (let i = 0; i < log.length; i = i + 1) {
    console.log("r" + (i < 10 ? "0" : "") + i + "=" + log[i]);
  }
  console.log("count=" + log.length);
});

console.log("sync-tail=reached");
