import events, { EventEmitter } from "node:events";

console.log(EventEmitter.errorMonitor === events.errorMonitor, typeof events.errorMonitor);
console.log(String(events.errorMonitor));
const e = new EventEmitter();
console.log(e.getMaxListeners(), EventEmitter.getMaxListeners(e));
const f = () => {};
e.on("x", f);
console.log(EventEmitter.getEventListeners(e, "x").length);

e.on(events.errorMonitor, (err: any) => console.log("monitor", err.message));
console.log("error listeners", e.listenerCount("error"), e.listenerCount(events.errorMonitor));
try {
  e.emit("error", new Error("boom1"));
} catch (x: any) {
  console.log("caught", x.message);
}
e.on("error", (err: any) => console.log("handler", err.message));
console.log(e.emit("error", new Error("boom2")));
const m = new EventEmitter();
m.once(events.errorMonitor, () => console.log("once monitor"));
m.on("error", () => console.log("h"));
m.emit("error", new Error("a"));
m.emit("error", new Error("b"));
console.log(m.listenerCount(events.errorMonitor));
