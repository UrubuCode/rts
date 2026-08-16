// Cross-runtime: EventTarget composes once listeners, AbortSignal removal, and dispatch order.
const target = new EventTarget();
const controller = new AbortController();
const seen: string[] = [];
target.addEventListener("ping", () => seen.push("normal"));
target.addEventListener("ping", () => seen.push("once"), { once: true });
target.addEventListener("ping", () => seen.push("abortable"), { signal: controller.signal });
target.dispatchEvent(new Event("ping"));
controller.abort();
target.dispatchEvent(new Event("ping"));
console.log(seen.join(","));

