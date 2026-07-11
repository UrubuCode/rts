// node:diagnostics_channel — in-process pub/sub bus.
import { describe, test, expect } from "rts:test";
import { channel, hasSubscribers, subscribe, unsubscribe } from "node:diagnostics_channel";

// --- module-level subscribe + Channel.publish -------------------------------
let received = "";
let count = 0;
const ch = channel("my:channel");
const chName = ch.name;
const hasBefore = hasSubscribers("my:channel");

subscribe("my:channel", (msg: string, name: string) => {
    received = msg;
    count = count + 1;
});
const hasAfter = hasSubscribers("my:channel");
const chHasGetter = ch.hasSubscribers;

ch.publish("hello");
const receivedOk = received === "hello" && count === 1;

ch.publish("again");
const countOk = count === 2;

// --- channel.subscribe + name propagation to publish ------------------------
let secondName = "";
const ch2 = channel("other:channel");
ch2.subscribe((msg: string, name: string) => { secondName = name; });
ch2.publish("x");
const nameForwardedOk = secondName === "other:channel";

// --- unsubscribe: returns false when nothing matches ------------------------
// (Reference-identity removal is limited by the engine reifying each PolyValue
// function argument to a fresh handle — see the module doc — so a subscriber
// cannot currently be matched back by a later reference. The false-path is the
// reliably observable behavior.)
function noopSub(msg: string) {}
const unknownUnsub = unsubscribe("never:subscribed", noopSub);

describe("node:diagnostics_channel", () => {
    test("channel.name", () => expect(chName).toBe("my:channel"));
    test("hasSubscribers before", () => expect(hasBefore).toBe(false));
    test("hasSubscribers after subscribe", () => expect(hasAfter).toBe(true));
    test("channel.hasSubscribers getter", () => expect(chHasGetter).toBe(true));
    test("publish delivers message", () => expect(receivedOk).toBe(true));
    test("publish twice", () => expect(countOk).toBe(true));
    test("channel.subscribe + name forwarded", () => expect(nameForwardedOk).toBe(true));
    test("unsubscribe unknown false", () => expect(unknownUnsub).toBe(false));
});
