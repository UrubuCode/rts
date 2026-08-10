// node:timers/promises — the promise-returning timers, and what each PINS.
//
// Every assertion here is about an OBSERVABLE the callback form cannot give:
// that the promise fulfils with the value it was handed, that time actually
// passed before it did, that an interval yields repeatedly and stops on
// `break`, and that an aborted signal REJECTS rather than fulfilling late.
import { describe, test, expect } from "rts:test";
import { setTimeout as sleep, setImmediate as soon, setInterval as every, scheduler } from "node:timers/promises";

const started = Date.now();
const slept = await sleep(60, "slept");
const elapsed = Date.now() - started;

const immediate = await soon("soon");
const waited = await scheduler.wait(10);
const yielded = await scheduler.yield();

// A `for await` over an interval: it yields forever, so the loop is what ends
// it. Three ticks, then `break` — which calls the iterator's `return()`.
const ticks: string[] = [];
for await (const tick of every(20, "tick")) {
    ticks.push(tick);
    if (ticks.length === 3) break;
}

// An abort must reject. Fulfilling late would be the failure this pins: the
// timer is five seconds out and the test does not take five seconds.
const controller = new AbortController();
const abandoned = sleep(5000, "never", { signal: controller.signal });
controller.abort();
let rejected = "not rejected";
try {
    await abandoned;
} catch (error) {
    rejected = error.name;
}

describe("node:timers/promises", () => {
    test("setTimeout fulfils with its value", () => expect(slept).toBe("slept"));
    test("setTimeout waits the delay", () => expect(elapsed >= 50).toBe(true));
    test("setImmediate fulfils with its value", () => expect(immediate).toBe("soon"));
    test("scheduler.wait fulfils with undefined", () => expect(waited).toBe(undefined));
    test("scheduler.yield fulfils with undefined", () => expect(yielded).toBe(undefined));
    test("setInterval yields until the loop breaks", () => expect(ticks.length).toBe(3));
    test("setInterval yields the value each tick", () => expect(ticks[2]).toBe("tick"));
    test("an aborted signal rejects", () => expect(rejected).toBe("AbortError"));
});
