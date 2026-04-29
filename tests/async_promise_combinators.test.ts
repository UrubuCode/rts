import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Promise.all — aguarda todas
const all = await Promise.all([
  Promise.resolve(1),
  Promise.resolve(2),
  Promise.resolve(3),
]);
print(all.join(","));  // 1,2,3

// Promise.all com async functions
async function double(n: number): Promise<number> {
  return n * 2;
}
const doubled = await Promise.all([double(1), double(2), double(3)]);
print(doubled.join(","));  // 2,4,6

// Promise.allSettled — não rejeita se uma falhar
const settled = await Promise.allSettled([
  Promise.resolve("ok"),
  Promise.reject(new Error("fail")),
  Promise.resolve("also ok"),
]);
for (const r of settled) {
  if (r.status === "fulfilled") {
    print(`fulfilled: ${r.value}`);
  } else {
    print(`rejected: ${r.reason.message}`);
  }
}

// Promise.race — retorna o primeiro
async function delay(ms: number, val: string): Promise<string> {
  return val;  // RTS é síncrono, todos resolvem "imediatamente"
}
const winner = await Promise.race([
  delay(100, "slow"),
  delay(10, "fast"),
  delay(50, "medium"),
]);
print(`race: ${winner}`);  // race: slow (primeiro na lista = primeiro a resolver em sync)

// Promise.any — primeiro que resolve (ignora rejeições)
const any = await Promise.any([
  Promise.reject(new Error("e1")),
  Promise.resolve("winner"),
  Promise.resolve("also fine"),
]);
print(`any: ${any}`);  // any: winner

describe("async_promise_combinators", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "1,2,3\n2,4,6\nfulfilled: ok\nrejected: fail\nfulfilled: also ok\nrace: slow\nany: winner\n"
  ));
});
