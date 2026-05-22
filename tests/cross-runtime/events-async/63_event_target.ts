// Cross-runtime compatibility: EventTarget dispatch ordering.
const target = new EventTarget();
const log: string[] = [];

target.addEventListener("ping", (ev: Event) => {
  log.push("a:" + ev.type);
});
target.addEventListener("ping", () => {
  log.push("b");
});

const ok = target.dispatchEvent(new Event("ping"));
console.log("ok=" + ok);
console.log("log=" + log.join(","));
