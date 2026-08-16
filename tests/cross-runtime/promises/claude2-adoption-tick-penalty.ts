// Cross-runtime: ADOPTION costs ticks. Resolving a promise with another promise
// -- from the executor or by returning one from a handler -- settles two ticks
// later than resolving with a plain value. A thenable costs a different amount.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const marks: string[] = [];

// The ruler: 14 already-scheduled ticks. A mark printed between t3 and t4
// landed on tick 4.
let ruler: Promise<void> = Promise.resolve();
for (let i = 1; i <= 14; i++) {
  const k = i;
  ruler = ruler.then(function () { marks.push("t" + k); });
}

const donor = Promise.resolve("D");

// 1) executor resolves with a plain value
new Promise(function (r: any) { r("plain"); }).then(function () { marks.push("execValue"); });

// 2) executor resolves with a native promise
new Promise(function (r: any) { r(donor); }).then(function () { marks.push("execPromise"); });

// 3) executor resolves with a thenable
new Promise(function (r: any) { r({ then: function (rr: any) { rr("t"); } }); }).then(function () { marks.push("execThenable"); });

// 4) a handler that returns a plain value
Promise.resolve().then(function () { return "v"; }).then(function () { marks.push("handlerValue"); });

// 5) a handler that returns a native promise
Promise.resolve().then(function () { return donor; }).then(function () { marks.push("handlerPromise"); });

// 6) a handler that returns a thenable
Promise.resolve().then(function () { return { then: function (rr: any) { rr("t"); } }; }).then(function () { marks.push("handlerThenable"); });

// 7) two adoptions in a row stack
Promise.resolve().then(function () { return Promise.resolve(donor); }).then(function () { marks.push("doubleAdopt"); });

// 8) rejection adopts too: returning a rejected promise from a handler
Promise.resolve().then(function () { return Promise.reject("R"); }).then(function () { marks.push("never"); }, function () { marks.push("rejectedAdopt"); });

// 9) a plain rejection, for comparison
Promise.resolve().then(function () { throw "T"; }).then(function () { marks.push("never2"); }, function () { marks.push("plainReject"); });

log("synchronousTail");

(async function () {
  for (let i = 0; i < 24; i++) await null;
  log("timeline=" + marks.join(","));

  function at(tag: string): number {
    const i = marks.indexOf(tag);
    if (i < 0) return -1;
    let ticks = 0;
    for (let k = 0; k < i; k++) if (marks[k].charAt(0) === "t" && marks[k].length <= 3) ticks++;
    return ticks;
  }

  log("execValueTick=" + at("execValue"));
  log("execPromiseTick=" + at("execPromise"));
  log("execThenableTick=" + at("execThenable"));
  log("handlerValueTick=" + at("handlerValue"));
  log("handlerPromiseTick=" + at("handlerPromise"));
  log("handlerThenableTick=" + at("handlerThenable"));
  log("doubleAdoptTick=" + at("doubleAdopt"));
  log("rejectedAdoptTick=" + at("rejectedAdopt"));
  log("plainRejectTick=" + at("plainReject"));

  log("adoptionCosts=" + (at("handlerPromise") - at("handlerValue")));
  log("execAdoptionCosts=" + (at("execPromise") - at("execValue")));
  log("stacking=" + (at("doubleAdopt") - at("handlerPromise")));
  log("rejectionAdoptionCosts=" + (at("rejectedAdopt") - at("plainReject")));
  log("thenableVsPromise=" + (at("handlerThenable") - at("handlerPromise")));

  // 10) awaiting the adopted promises still gives the right values
  log("donorValue=" + (await donor));
  log("chainValue=" + (await Promise.resolve().then(function () { return donor; })));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
