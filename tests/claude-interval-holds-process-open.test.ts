// `setInterval` keeps the program running, so an interval with nothing else
// pending actually ticks.
//
// It did not. `node:timers`' loop source answered `Pending::Blocked` for every
// periodic timer — pumped on each pass, contributing no deadline — so an
// interval fired only while something ELSE happened to keep the loop turning.
// A program whose only outstanding work was an interval ended at its last
// statement, measured on `1d070295c`:
//
//   $ rts run p1_interval.ts        -> "armed", rc=0, 0 ticks, immediately
//   $ node p1_interval.js           -> alive indefinitely (killed at 8s, rc=124)
//
// # Why the assertion is a parked `await` and not a callback count
//
// Because the defect's symptom is a CLEAN EXIT. A fixture that counted ticks in
// a `describe` block would read `0` and pass, since `describe` runs while the
// interval is still outstanding — the shape this repository calls a green
// vacuum. Parking on a promise the interval resolves makes the failure loud
// instead: with the interval not holding the loop open, nothing is left that
// could settle it, and `promise_await`'s deadlock detector raises
// "this promise cannot settle". Red, and in under a second.
//
// # Why the watchdog is UNREF'd
//
// A ref'd `setTimeout` is precisely the workaround this change removes: it holds
// the program open by itself, which would make the interval's own liveness
// unobservable and the fixture green on the broken engine. Unrefed, it is still
// pumped on every pass — so it bounds the wait if the interval stops firing for
// some other reason — while counting for nothing.
import { describe, test, expect } from "rts:test";

const ticks: number[] = [];
let finish: (value: number) => void = () => {};
const ticked: Promise<number> = new Promise((resolve: any) => {
  finish = resolve;
});

const watchdog: any = setTimeout(() => {
  throw new Error("the interval stopped after " + ticks.length + " ticks in 6s");
}, 6000);
watchdog.unref();

// 20ms, not 1000: the question is whether a periodic timer is in the liveness
// accounting at all, and that answer does not depend on the period. A long one
// would only make the fixture slow.
const handle = setInterval(() => {
  ticks.push(ticks.length);
  if (ticks.length === 3) {
    clearInterval(handle);
    finish(ticks.length);
  }
}, 20);

const counted = await ticked;
clearTimeout(watchdog);

describe("setInterval holds the program open", () => {
  test("an interval with nothing else pending fires repeatedly", () => {
    expect(counted).toBe(3);
    expect(ticks.length).toBe(3);
  });

  test("clearInterval then lets the program end", () => {
    // Pinned by this file TERMINATING rather than by a value: a cleared
    // interval that stayed in the `In` set would leave nothing to end the
    // program, and `rts test` would kill the child at `RTS_TEST_TIMEOUT` and
    // report it failed. The opposite defect to the one above, and the worse
    // one — a program that never finishes.
    expect(typeof handle).toBe("object");
  });
});
