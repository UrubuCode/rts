// Cross-runtime compatibility: AbortController and signal events.
const controller = new AbortController();
const seen: string[] = [];

controller.signal.addEventListener("abort", () => {
  seen.push("event:" + String(controller.signal.aborted));
});

console.log("before=" + controller.signal.aborted);
controller.abort("stop");
console.log("after=" + controller.signal.aborted);
console.log("reason=" + String(controller.signal.reason));
console.log("seen=" + seen.join(","));
