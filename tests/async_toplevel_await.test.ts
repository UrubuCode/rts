import { describe, test, expect } from "rts:test";

// RTS executa tudo sincronamente — top-level await deve funcionar
// como se fosse um runtime com event loop implícito (como Bun/Node ESM)
let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Top-level await — deve bloquear e continuar
async function resolveValue(n: number): Promise<number> {
  return n * 2;
}

// Top-level await de função async
const a = await resolveValue(21);
print(`${a}`);  // 42

// Cadeia de awaits no top-level
const b = await resolveValue(await resolveValue(5));
print(`${b}`);  // 20

// await de valor já resolvido
const c = await 99;
print(`${c}`);  // 99

// async/await com try/catch no top-level
async function mayFail(fail: boolean): Promise<string> {
  if (fail) throw new Error("async error");
  return "ok";
}

try {
  const d = await mayFail(false);
  print(d);  // ok
} catch (e) {
  print("should not reach");
}

try {
  await mayFail(true);
  print("should not reach");
} catch (e) {
  print(`caught: ${(e as Error).message}`);  // caught: async error
}

describe("async_toplevel_await", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "42\n20\n99\nok\ncaught: async error\n"
  ));
});
