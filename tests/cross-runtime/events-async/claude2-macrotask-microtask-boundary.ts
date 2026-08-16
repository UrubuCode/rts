// Cross-runtime: the microtask queue is drained COMPLETELY before the next
// macrotask runs, and two timers of the same delay fire in scheduling order.
// Every chain here has a fixed, finite length, so nothing races.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const steps: string[] = [];

// 1) two timers of equal delay, scheduled in order
setTimeout(function () {
  steps.push("timer1");
  // a microtask queued from a macrotask runs before the NEXT macrotask
  queueMicrotask(function () { steps.push("t1-micro"); });
  Promise.resolve().then(function () { steps.push("t1-then"); });
}, 0);

setTimeout(function () {
  steps.push("timer2");
  queueMicrotask(function () { steps.push("t2-micro"); });
}, 0);

// 2) a bounded microtask ladder queued from the top level: all four rungs run
//    before timer1
let rung: Promise<void> = Promise.resolve();
for (let i = 1; i <= 4; i++) {
  const k = i;
  rung = rung.then(function () { steps.push("micro" + k); });
}

// 3) a queueMicrotask ladder of the same shape
function ladder(left: number) {
  queueMicrotask(function () {
    steps.push("qm" + (5 - left));
    if (left > 1) ladder(left - 1);
  });
}
ladder(4);

steps.push("sync");

// 4) the last timer reports the whole timeline and ends the program. It is
//    scheduled last, so it runs last.
setTimeout(function () {
  steps.push("timer3");
  log("timeline=" + steps.join(","));
  log("syncFirst=" + (steps[0] === "sync"));
  log("allMicroBeforeTimer1=" + (steps.indexOf("micro4") < steps.indexOf("timer1")));
  log("allQmBeforeTimer1=" + (steps.indexOf("qm4") < steps.indexOf("timer1")));
  log("timerOrder=" + (steps.indexOf("timer1") < steps.indexOf("timer2")) + "," + (steps.indexOf("timer2") < steps.indexOf("timer3")));
  log("timer1MicrosBeforeTimer2=" + (steps.indexOf("t1-then") < steps.indexOf("timer2")));
  log("timer2MicroBeforeTimer3=" + (steps.indexOf("t2-micro") < steps.indexOf("timer3")));

  // 5) setTimeout returns something truthy that clearTimeout accepts, and a
  //    cleared timer never fires
  let clearedRan = false;
  const handle = setTimeout(function () { clearedRan = true; }, 0);
  clearTimeout(handle);
  log("handleType=" + (typeof handle === "object" || typeof handle === "number"));

  // 6) extra arguments are forwarded to the callback
  let forwarded = "unset";
  setTimeout(function (a: any, b: any) { forwarded = a + "/" + b + "/" + arguments.length; }, 0, "x", "y");

  // 7) a timer scheduled from INSIDE a timer runs after the ones already
  //    queued for the same tick
  const late: string[] = [];
  setTimeout(function () { late.push("outer"); setTimeout(function () { late.push("nested"); }, 0); }, 0);
  setTimeout(function () { late.push("sibling"); }, 0);

  // 8) a final timer confirms all of it, and closes the file
  setTimeout(function () {
    log("clearedRan=" + clearedRan);
    log("forwardedArgs=" + forwarded);
    // this timer was scheduled BEFORE the nested one existed, so it runs first
    log("lateSoFar=" + late.join(","));
    log("stepCount=" + steps.length);

    // 9) clearing an already-fired handle is a quiet no-op, as is clearing
    //    nothing at all
    log("clearFired=" + (function () {
      try { clearTimeout(handle); clearTimeout(undefined as any); return "quiet"; }
      catch (e: any) { return e.constructor.name; }
    })());

    // 10) a string delay coerces, and a bounded microtask still goes first
    const tailMarks: string[] = [];
    Promise.resolve().then(function () { tailMarks.push("micro"); });
    setTimeout(function () {
      tailMarks.push("stringDelay");
      log("stringDelay=" + tailMarks.join(","));
      // by now the timer scheduled from inside another timer has run
      log("lateFinal=" + late.join(","));
      console.log("end");
    }, "0" as any);
  }, 0);
}, 0);
