import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const ee = new EventEmitter();

let called = 0;
function onData(arg: number): number {
  called = called + arg;
  return 0;
}

ee.on("data", onData);
const countAfterOn = ee.listenerCount("data");
print(countAfterOn === 1 ? "count_ok" : "count_fail");

ee.emit("data", 42);
const calledAfter42 = called;
print(calledAfter42 === 42 ? "emit_ok" : "emit_fail");

ee.emit("data", 8);
const calledAfter50 = called;
print(calledAfter50 === 50 ? "emit2_ok" : "emit2_fail");

let onceFired = 0;
function onOnce(arg: number): number {
  onceFired = onceFired + 1;
  return 0;
}
ee.once("ping", onOnce);
ee.emit("ping", 0);
ee.emit("ping", 0);
const onceOk = onceFired === 1;
print(onceOk ? "once_ok" : "once_fail");

ee.off("data", onData);
const countAfterOff = ee.listenerCount("data");
print(countAfterOff === 0 ? "off_ok" : "off_fail");

const had = ee.emit("data", 1);
const noListenersOk = !had;
print(noListenersOk ? "no_listeners_ok" : "no_listeners_fail");

ee.on("x", onData);
ee.on("x", onOnce);
ee.removeAllListeners("x");
const removeAllOk = ee.listenerCount("x") === 0;
print(removeAllOk ? "remove_all_ok" : "remove_all_fail");

const ae = new EventEmitter(true);
ae.on("data", onData);
const asyncCountOk = ae.listenerCount("data") === 1;
print(asyncCountOk ? "async_count_ok" : "async_count_fail");
ae.emit("data", 1);
print("async_emit_no_crash");

describe("eventemitter_global", () => {
  test("on — listenerCount = 1",    () => expect(countAfterOn === 1).toBe(true));
  test("emit acumula 42",           () => expect(calledAfter42 === 42).toBe(true));
  test("emit acumula 50",           () => expect(calledAfter50 === 50).toBe(true));
  test("once — dispara exatamente 1x", () => expect(onceOk).toBe(true));
  test("off — remove listener",     () => expect(countAfterOff === 0).toBe(true));
  test("emit sem listeners — false",() => expect(noListenersOk).toBe(true));
  test("removeAllListeners",        () => expect(removeAllOk).toBe(true));
  test("EventEmitter async — sem crash", () => expect(asyncCountOk).toBe(true));

  test("saida capturada completa", () =>
    expect(out).toBe(
      "count_ok\nemit_ok\nemit2_ok\nonce_ok\noff_ok\n" +
      "no_listeners_ok\nremove_all_ok\nasync_count_ok\nasync_emit_no_crash\n"
    ));
});
