import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Optional chaining com ?? (nullish coalescing)
type Config = {
  timeout?: number;
  server?: { host?: string; port?: number };
};

const cfg: Config = { server: { host: "localhost" } };
print(`${cfg?.server?.host ?? "unknown"}`);    // localhost
print(`${cfg?.server?.port ?? 3000}`);         // 3000
print(`${cfg?.timeout ?? 5000}`);              // 5000

// Optional method call com resultado
class Formatter {
  format(n: number): string {
    return `[${n}]`;
  }
}

const fmt: Formatter | null = new Formatter();
const noFmt: Formatter | null = null;

print(`${fmt?.format(42)}`);    // [42]
print(`${noFmt?.format(42)}`);  // undefined

// Optional chaining em array de objetos
const users: Array<{ name: string; address?: { city: string } }> = [
  { name: "Alice", address: { city: "NY" } },
  { name: "Bob" },
];

for (const u of users) {
  print(`${u.name}: ${u.address?.city ?? "no city"}`);
}

// Chained optional calls
type Chain = {
  next?: () => Chain;
  value?: number;
};

const chain: Chain = {
  value: 1,
  next: () => ({ value: 2, next: () => ({ value: 3 }) }),
};

print(`${chain?.value}`);              // 1
print(`${chain?.next?.()?.value}`);   // 2
print(`${chain?.next?.()?.next?.()?.value}`);  // 3

const noChain: Chain | null = null;
print(`${noChain?.next?.()?.value ?? "none"}`);  // none

describe("optional_chain_advanced", () => {
  test("combined", () => expect(__rtsCapturedOutput).toBe(
    "localhost\n3000\n5000\n[42]\nundefined\nAlice: NY\nBob: no city\n1\n2\n3\nnone\n"
  ));
});
