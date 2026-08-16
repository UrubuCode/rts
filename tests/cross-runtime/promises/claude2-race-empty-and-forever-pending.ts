// Cross-runtime: Promise.race over an EMPTY iterable is pending forever, and a
// race whose entries never settle stays pending too. Nothing here hangs: the
// evidence is that a flag is still unset after a deep microtask drain.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const marks: string[] = [];

// 1) race([]) -- a promise that will never settle
const empty = Promise.race([]);
empty.then(function () { marks.push("empty-fulfilled"); }, function () { marks.push("empty-rejected"); });
log("emptyIsPromise=" + (empty instanceof Promise));

// 2) race over entries that never settle
const never1 = new Promise(function () { });
const never2 = new Promise(function () { });
const stuck = Promise.race([never1, never2]);
stuck.then(function () { marks.push("stuck-settled"); }, function () { marks.push("stuck-rejected"); });

// 3) race with one settled entry among pending ones takes the settled one
const firstWins = Promise.race([Promise.resolve("A"), never1, Promise.resolve("B")]);

// 4) input ORDER decides among entries that are all already settled
const allSettledInputs = Promise.race([Promise.resolve("x"), Promise.reject("y")]);

// 5) a rejection can win the race
const rejWins = Promise.race([Promise.reject("R"), Promise.resolve("F")]);

// 6) a plain value in the list wins over a pending promise
const plainWins = Promise.race([never1, 99]);

// 7) a thenable entry is adopted, and costs the extra tick a thenable costs
const thenableWins = Promise.race([{ then: function (r: any) { r("T"); } }, Promise.resolve("N")]);

// 8) the result is always a NEW promise, never one of the inputs
const inputP = Promise.resolve("same");
log("resultIsNotInput=" + (Promise.race([inputP]) !== inputP));

// 9) race of a single pending promise is pending; adopting it later would
//    settle it, but nothing here ever does
log("stuckIsNotNever1=" + (stuck !== never1));

(async function () {
  const results: string[] = [];
  firstWins.then(function (v: any) { results.push("first=" + v); });
  allSettledInputs.then(function (v: any) { results.push("allSettled=" + v); }, function (e: any) { results.push("allSettledRej=" + e); });
  rejWins.then(function (v: any) { results.push("rej=" + v); }, function (e: any) { results.push("rejRej=" + e); });
  plainWins.then(function (v: any) { results.push("plain=" + v); });
  thenableWins.then(function (v: any) { results.push("thenable=" + v); });

  // drain far past anything the settlements above could need
  for (let i = 0; i < 20; i++) await null;

  log("firstSettledWins=" + results[0]);
  log("orderAmongSettled=" + results[1]);
  log("rejectionWins=" + results[2]);
  log("plainValueWins=" + results[3]);
  log("thenableAdopted=" + results[4]);
  log("resultCount=" + results.length);

  // 10) after all that, neither forever-pending promise has settled
  log("marksAfterDrain=" + JSON.stringify(marks.join(",")));

  // 11) and they are still ordinary pending promises: a fresh then does not
  //     fire either, while the same then on a settled promise does
  let late = "not-called";
  empty.then(function () { late = "called"; }, function () { late = "called"; });
  Promise.resolve().then(function () { late = late === "not-called" ? "still-not-called" : late; });
  for (let i = 0; i < 6; i++) await null;
  log("lateThenOnEmpty=" + late);

  // 12) race([]) is not the same object twice
  log("freshEachCall=" + (Promise.race([]) !== Promise.race([])));

  // 13) race over other empty iterables is pending just the same
  let emptySetSettled = "pending";
  Promise.race(new Set()).then(function () { emptySetSettled = "fulfilled"; }, function () { emptySetSettled = "rejected"; });
  let emptyGenSettled = "pending";
  function* nothing() { }
  Promise.race(nothing()).then(function () { emptyGenSettled = "fulfilled"; }, function () { emptyGenSettled = "rejected"; });
  for (let i = 0; i < 8; i++) await null;
  log("emptySet=" + emptySetSettled + " emptyGenerator=" + emptyGenSettled);

  // 14) a NON-iterable argument rejects the result rather than throwing
  let threw = "no";
  let nonIterable: any;
  try { nonIterable = Promise.race({} as any); } catch (e: any) { threw = "threw"; }
  log("nonIterableThrew=" + threw + " isPromise=" + (nonIterable instanceof Promise));
  log("nonIterableSettles=" + (await nonIterable.then(function () { return "fulfilled"; }, function (e: any) { return "rejected:" + e.constructor.name; })));

  // 15) race over a string iterates its characters, so the first one wins
  log("overString=" + (await Promise.race("xyz")));

  // 16) race over a Set of settled promises takes insertion order
  log("overSet=" + (await Promise.race(new Set([Promise.resolve("s1"), Promise.resolve("s2")]))));

  // 17) an iterable whose Symbol.iterator throws rejects the result
  const badIterable: any = { [Symbol.iterator]: function () { throw new RangeError("no"); } };
  log("badIterable=" + (await Promise.race(badIterable).then(function () { return "fulfilled"; }, function (e: any) { return "rejected:" + e.constructor.name; })));

  console.log("end");
})();
