import { describe, test, expect } from "rts:test";
import {
  channel, hasSubscribers, subscribe, unsubscribe, tracingChannel,
  Channel, TracingChannel,
} from "node:diagnostics_channel";

// ---- channel + publish/subscribe -------------------------------------------
const got: any[] = [];
const ch: any = channel("test:ch");
const isChannel = ch instanceof Channel;
const before = ch.hasSubscribers;
const onMsg = (msg: any, name: any) => { got.push(name + ":" + msg); };
ch.subscribe(onMsg);
const after = ch.hasSubscribers;
ch.publish("hi");
ch.publish("bye");

// ---- same-name returns same channel ----------------------------------------
const sameChannel = channel("test:ch") === ch;

// ---- module-level hasSubscribers/subscribe -------------------------------
const modBefore = hasSubscribers("test:mod");
const modGot: any[] = [];
const modFn = (m: any) => { modGot.push(m); };
subscribe("test:mod", modFn);
const modAfter = hasSubscribers("test:mod");
channel("test:mod").publish("x");

// ---- unsubscribe of a never-subscribed handler returns false ---------------
// (removal-by-reference across a call boundary needs stable function-value
// identity, an engine gap — see mod.rs; unsubscribe still returns a bool.)
const neverFn = (m: any) => {};
const unsubMissing = ch.unsubscribe(neverFn);

// ---- TracingChannel traceSync (success + error) ----------------------------
const events: any[] = [];
const tc: any = tracingChannel("op");
const isTC = tc instanceof TracingChannel;
tc.subscribe({
  start: (c: any) => { events.push("start"); },
  end: (c: any) => { events.push("end"); },
  error: (c: any) => { events.push("error"); },
});
const traceOut = tc.traceSync(() => 42);
const ctxObj: any = {};
let threw = false;
try {
  tc.traceSync(() => { throw new Error("boom"); }, ctxObj);
} catch (e) { threw = true; }

// ---- TracingChannel sub-channels are real Channels -------------------------
const startIsChannel = tc.start instanceof Channel;
const tcHasSubs = tc.hasSubscribers;

describe("node:diagnostics_channel", () => {
  test("channel + publish/subscribe", () => {
    expect(isChannel).toBe(true);
    expect(before).toBe(false);
    expect(after).toBe(true);
    expect(got.length).toBe(2);
    expect(got[0]).toBe("test:ch:hi");
    expect(got[1]).toBe("test:ch:bye");
  });
  test("same name = same channel", () => { expect(sameChannel).toBe(true); });
  test("module-level fns", () => {
    expect(modBefore).toBe(false);
    expect(modAfter).toBe(true);
    expect(modGot.length).toBe(1);
    expect(modGot[0]).toBe("x");
  });
  test("unsubscribe missing handler returns false", () => {
    expect(unsubMissing).toBe(false);
  });
  test("TracingChannel traceSync", () => {
    expect(isTC).toBe(true);
    expect(traceOut).toBe(42);
    expect(events[0]).toBe("start");
    expect(events[1]).toBe("end");
    expect(threw).toBe(true);
    expect(ctxObj.error).not.toBe(undefined);
  });
  test("TracingChannel error event on throw", () => {
    expect(events.indexOf("error") >= 0).toBe(true);
  });
  test("sub-channels are Channels", () => {
    expect(startIsChannel).toBe(true);
    expect(tcHasSubs).toBe(true);
  });
});
