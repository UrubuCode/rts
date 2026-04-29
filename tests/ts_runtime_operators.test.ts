import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Non-null assertion `!` — apagado pelo parser, equivale a acesso direto.
const value: string | null = "hello";
print(`${value!.length}`);

// `as const` — narrowing de tipo, sem efeito em runtime alem do strip.
const arr = [1, 2, 3] as const;
print(`${arr[0]}`);
print(`${arr[2]}`);

// `as` cast — strip transparente.
const n = 42 as number;
print(`${n}`);

// `satisfies` — strip transparente.
const cfg = { port: 3000 } satisfies { port: number };
print(`${cfg.port}`);

describe("ts_runtime_operators", () => {
  test("strip_to_runtime", () => expect(__rtsCapturedOutput).toBe(
    "5\n1\n3\n42\n3000\n"
  ));
});
