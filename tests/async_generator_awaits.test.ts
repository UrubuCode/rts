import { describe, test, expect } from "rts:test";

// An `async function*` parks at `yield` and DRAINS at `await`: its frame is stepped by
// its own `next()`, so a promise reaction resuming it too could not be told apart.
// These are the shapes that ran only on the running emitter until the MIR stage
// learned the draining form.

const later = (v: any) => new Promise((r) => setTimeout(() => r(v), 1));

async function* counter(n: number, log: string[]) {
  for (let i = 0; i < n; i++) {
    const v = await later(i * 10);
    log.push("made " + v);
    yield v;
  }
  return "end";
}

async function* rejects(log: string[]) {
  try {
    await Promise.reject(new Error("boom"));
  } catch (e) {
    yield "caught " + (e as Error).message;
  } finally {
    log.push("fin");
  }
}

async function* overSync() {
  for await (const x of [1, Promise.resolve(2), 3]) yield x * 2;
}

describe("an async generator", () => {
  test("awaits between yields, and is stepped by for await", async () => {
    const log: string[] = [];
    const out: number[] = [];
    for await (const v of counter(3, log)) out.push(v as number);
    expect(out.join()).toBe("0,10,20");
    expect(log.join()).toBe("made 0,made 10,made 20");
  });
  test("a rejected await raises where it was written", async () => {
    const log: string[] = [];
    const out: string[] = [];
    for await (const v of rejects(log)) out.push(v);
    expect(out.join() + "|" + log.join()).toBe("caught boom|fin");
  });
  test("for await over a sync source inside one", async () => {
    const out: number[] = [];
    for await (const v of overSync()) out.push(v);
    expect(out.join()).toBe("2,4,6");
  });
  test("return early", async () => {
    const g = counter(5, []);
    expect((await g.next()).value).toBe(0);
    expect(JSON.stringify(await g.return("early"))).toBe('{"value":"early","done":true}');
  });
});
