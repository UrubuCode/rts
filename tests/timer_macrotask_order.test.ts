import { describe, test, expect } from "rts:test";
// time.sleep_ms fica: e' uma pausa SINCRONA que drena o loop, e o unico
// par padrao — `await new Promise(r => setTimeout(r, n))` — hoje nao
// settla neste motor ("this promise cannot settle"), o que apagaria a
// ordem que este ficheiro existe para fixar.
import { time } from "rts";

// Regression (cross-runtime #207 timer ordering): setTimeout(0) e setImmediate
// rodavam em THREADS paralelas — ordem nao-deterministica vs microtasks.
// Fix: setTimeout(0) vira macrotask na thread do main (drena APOS microtasks);
// setImmediate enfileira (sem thread) e roda na check phase. Ordem JS spec:
// sync -> microtask -> immediate -> timeout, deterministico.

let out = "";
function print(v: string): void { out += v + "\n"; }

const order: string[] = [];
order.push("sync");
queueMicrotask(() => order.push("micro"));
setImmediate(() => order.push("immediate"));
setTimeout(() => order.push("timeout"), 0);

// sleep eh ponto de quiescencia: drena microtask/immediate/macrotask.
time.sleep_ms(5);

print(order.join("|"));   // sync|micro|immediate|timeout

// setImmediate dispara antes do fim (via drain no sleep)
let fired = 0;
setImmediate(() => { fired = fired + 1; });
time.sleep_ms(5);
print("fired=" + fired);  // fired=1

describe("timer macrotask order (#207)", () => {
  test("sync->micro->immediate->timeout deterministico", () =>
    expect(out).toBe("sync|micro|immediate|timeout\nfired=1\n"));
});
