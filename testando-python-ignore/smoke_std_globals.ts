// console (→io), performance, headers, form_data, event_target, abort
console.log("console-ok");
console.error("err-ok");
const t = performance.now();
console.log("perf:" + (t >= 0));

const h = new Headers();
h.set("Content-Type", "text/plain");
h.append("X-A", "1");
console.log("headers:" + h.get("content-type") + "," + h.has("x-a"));

const fd = new FormData();
fd.append("k", "v");
console.log("formdata:" + fd.get("k"));

const et = new EventTarget();
let fired = 0;
et.addEventListener("ping", () => { fired = 1; });
et.dispatchEvent(new Event("ping"));
console.log("eventtarget-fired:" + fired);

const ac = new AbortController();
console.log("abort-before:" + ac.signal.aborted);
ac.abort("stop");
console.log("abort-after:" + ac.signal.aborted);
