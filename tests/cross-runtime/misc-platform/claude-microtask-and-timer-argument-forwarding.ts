// Cross-runtime: the ordering contract between synchronous code, the microtask
// queue (queueMicrotask and promise reactions share ONE queue, in FIFO order)
// and a zero-delay timer — plus setTimeout forwarding its extra arguments and
// clearTimeout accepting its own handle more than once.

const log: string[] = [];

console.log("setTimeout_length=" + (setTimeout.length >= 1));

// Interleaving queueMicrotask with promise reactions: one queue, FIFO.
queueMicrotask(function () {
  log.push("qm1");
});
Promise.resolve().then(function () {
  log.push("p1");
});
queueMicrotask(function () {
  log.push("qm2");
  queueMicrotask(function () {
    log.push("qm2-nested");
  });
});
Promise.resolve().then(function () {
  log.push("p2");
  return Promise.resolve();
}).then(function () {
  log.push("p2-chained");
});
queueMicrotask(function () {
  log.push("qm3");
});

// A thenable takes an extra turn compared to a plain value.
Promise.resolve({
  then: function (resolve: (v: unknown) => void) {
    log.push("thenable-called");
    resolve(1);
  },
}).then(function () {
  log.push("thenable-settled");
});

// A rejection handled with catch joins the same queue.
Promise.reject(new Error("x")).catch(function () {
  log.push("rejected");
});

// await in an async function is a microtask boundary.
(async function (): Promise<void> {
  log.push("async-sync");
  await undefined;
  log.push("after-await-1");
  await undefined;
  log.push("after-await-2");
})();

// Timers run after the whole microtask queue is drained.
const handle = setTimeout(function (a: unknown, b: unknown, c: unknown) {
  log.push("t0:" + String(a) + "," + String(b) + "," + String(c));
  finish();
}, 0, "x", 7, undefined);

const second = setTimeout(function () {
  log.push("t1-should-not-run");
}, 0);
clearTimeout(second);
clearTimeout(second);
clearTimeout(second);
clearTimeout(undefined as any);
clearTimeout(null as any);
clearTimeout(999999 as any);
clearTimeout("not-a-handle" as any);
console.log("clear_is_idempotent=true");

// A handle is truthy and survives being passed around; its concrete TYPE is
// deliberately not asserted, only that clearTimeout accepts it back.
console.log("handle_defined=" + (handle !== undefined) + " truthy=" + Boolean(handle));

// Extra arguments are forwarded verbatim, including none at all.
setTimeout(function () {
  log.push("no-args:" + arguments.length);
}, 0);
setTimeout(function (...rest: unknown[]) {
  log.push("rest:" + rest.length + ":" + rest.map(String).join("|"));
}, 0, 1, "two", null, false);

// A missing delay is the same as 0.
setTimeout(function () {
  log.push("no-delay");
});

// A microtask queued from INSIDE a timer runs before the next timer.
setTimeout(function () {
  log.push("timerA");
  queueMicrotask(function () {
    log.push("timerA-microtask");
  });
}, 0);
setTimeout(function () {
  log.push("timerB");
}, 0);

// setInterval repeats until it is cleared, and its handle works the same way.
let ticks = 0;
const interval = setInterval(function (label: unknown) {
  ticks++;
  log.push("interval:" + String(label) + ":" + ticks);
  if (ticks === 2) clearInterval(interval);
}, 0, "i");

// A timer scheduled from inside a timer lands in a LATER turn, never this one.
setTimeout(function () {
  log.push("outer");
  setTimeout(function () {
    log.push("inner");
  }, 0);
}, 0);

// A non-function first argument is refused rather than silently ignored.
try {
  (setTimeout as any)("log.push('never')", 0);
  console.log("string_callback=accepted");
} catch (e: any) {
  console.log("string_callback=" + e.constructor.name);
}
try {
  (setTimeout as any)();
  console.log("no_callback=accepted");
} catch (e: any) {
  console.log("no_callback=" + e.constructor.name);
}

log.push("sync-end");
console.log("nothing_ran_yet=" + JSON.stringify(log));

let finished = false;
function finish(): void {
  if (finished) return;
  finished = true;
  // Three turns of slack so the interval has been cleared and the timer
  // scheduled from inside a timer has had its own turn.
  setTimeout(function () {
    setTimeout(function () {
      setTimeout(report, 0);
    }, 0);
  }, 0);
}

function report(): void {
  const microtaskNames: string[] = ["qm1", "p1", "qm2", "qm2-nested", "qm3", "p2", "p2-chained", "thenable-called", "thenable-settled", "rejected", "after-await-1", "after-await-2"];
  const microtasks = log.filter(function (e) {
    return microtaskNames.indexOf(e) >= 0;
  });
  const first = function (prefix: string): string {
    const hit = log.filter(function (e) {
      return e.indexOf(prefix) === 0;
    });
    return hit.length > 0 ? hit[0] : "<absent>";
  };
  console.log("sync_first=" + (log[0] === "async-sync") + " " + (log[1] === "sync-end"));
  console.log("microtask_order=" + microtasks.join(">"));
  console.log("microtask_count=" + microtasks.length);
  console.log("all_microtasks_before_first_timer=" + (log.indexOf("p2-chained") < log.indexOf("t0:x,7,undefined")));
  console.log("timer_after_microtasks=" + (log.indexOf("t0:x,7,undefined") > log.indexOf("qm2-nested")));
  console.log("timer_args=" + first("t0:"));
  console.log("no_args=" + first("no-args:"));
  console.log("rest_args=" + first("rest:"));
  console.log("cleared_never_ran=" + (log.indexOf("t1-should-not-run") < 0));
  console.log("no_delay_ran=" + (log.indexOf("no-delay") >= 0));
  console.log("timer_microtask_before_next_timer=" + (log.indexOf("timerA-microtask") < log.indexOf("timerB")));
  console.log("timers_in_order=" + (log.indexOf("timerA") < log.indexOf("timerB")));
  console.log("interval_ran=" + log.filter(function (e) {
    return e.indexOf("interval:") === 0;
  }).join(" "));
  console.log("interval_repeated_then_stopped=" + (ticks === 2));
  console.log("nested_timer_later=" + (log.indexOf("inner") > log.indexOf("outer")));
  console.log("nested_after_all_first_round=" + (log.indexOf("inner") > log.indexOf("timerB")));
  console.log("total_entries=" + log.length);
}
