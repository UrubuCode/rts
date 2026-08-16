// Cross-runtime: what the combinators do when the ITERABLE misbehaves -- the
// iterator's next() throws mid-way, or Symbol.iterator is missing entirely.
// Focus: the combinator rejects rather than throwing, and already-pulled
// entries keep running.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

// An iterable that yields two promises and then throws on the third pull.
function badIterable(tag: string) {
  return {
    [Symbol.iterator]() {
      let i = 0;
      return {
        next() {
          i++;
          pulls.push(tag + ":next" + i);
          if (i <= 2) {
            return { value: Promise.resolve(tag + "-v" + i), done: false };
          }
          throw new RangeError("iterator gave up");
        },
        return() {
          pulls.push(tag + ":return");
          return { done: true, value: undefined };
        }
      };
    }
  };
}

// 1) Promise.all rejects with whatever next() threw; it does not throw
let threw1 = "no";
let a1: any;
try {
  a1 = Promise.all(badIterable("all") as any);
} catch (e: any) {
  threw1 = e.name;
}
log("allThrewSynchronously=" + threw1);
log("allReturnedPromise=" + (a1 instanceof Promise));
a1.then(
  function () { log("all fulfilled"); },
  function (e: any) { log("all rejected " + e.constructor.name); }
);

// 2) same for allSettled, any and race
Promise.allSettled(badIterable("settled") as any).then(
  function () { log("allSettled fulfilled"); },
  function (e: any) { log("allSettled rejected " + e.constructor.name); }
);
Promise.any(badIterable("any") as any).then(
  function () { log("any fulfilled"); },
  function (e: any) { log("any rejected " + e.constructor.name); }
);
Promise.race(badIterable("race") as any).then(
  function (v: any) { log("race fulfilled " + v); },
  function (e: any) { log("race rejected " + e.constructor.name); }
);

// 3) a non-iterable argument rejects with a TypeError, still not a throw
let threw3 = "no";
let a3: any;
try {
  a3 = Promise.all(42 as any);
} catch (e: any) {
  threw3 = e.name;
}
log("nonIterableThrewSynchronously=" + threw3);
a3.then(
  function () { log("nonIterable fulfilled"); },
  function (e: any) { log("nonIterable rejected " + e.constructor.name); }
);

// 4) Symbol.iterator present but not callable
const notCallable: any = { [Symbol.iterator]: 7 };
Promise.all(notCallable).then(
  function () { log("notCallable fulfilled"); },
  function (e: any) { log("notCallable rejected " + e.constructor.name); }
);

// 5) an iterator that is exhausted immediately gives an empty result
Promise.all([]).then(function (v: any) {
  log("emptyAll length=" + v.length + " isArray=" + Array.isArray(v));
});
Promise.allSettled([]).then(function (v: any) {
  log("emptyAllSettled length=" + v.length);
});

// 6) throwing on the FIRST pull: nothing was collected, and return() is never
//    called because the iterator itself signalled completion by throwing
const firstPull = {
  [Symbol.iterator]() {
    return {
      next() { pulls.push("first:next"); throw new EvalError("immediate"); },
      return() { pulls.push("first:return"); return { done: true, value: undefined }; }
    };
  }
};
Promise.all(firstPull as any).then(
  function () { log("firstPull fulfilled"); },
  function (e: any) { log("firstPull rejected " + e.constructor.name); }
);

// 7) the iterator is drained SYNCHRONOUSLY, so its throw beats a member
//    promise that was already rejected
const alreadyRejected = Promise.reject(new SyntaxError("member"));
alreadyRejected.catch(function () { /* keep it handled */ });
const mixed = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        pulls.push("mixed:next" + i);
        if (i === 1) return { value: alreadyRejected, done: false };
        throw new URIError("after a rejected member");
      }
    };
  }
};
Promise.all(mixed as any).then(
  function () { log("mixed fulfilled"); },
  function (e: any) { log("mixed rejected " + e.constructor.name); }
);

// 8) the pull log, from the tail of the queue
let tail: Promise<any> = Promise.resolve();
for (let i = 0; i < 12; i++) tail = tail.then(function () { return undefined; });
tail.then(function () {
  log("pulls=" + pulls.join("|"));
  log("pullCount=" + pulls.length);
  console.log("end");
});
