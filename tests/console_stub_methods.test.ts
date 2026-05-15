import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Esses metodos antes davam 'unknown namespace member'. Agora sao
// tratados como alias de io.print (stub) — output nao bate 100% com
// Bun/Node mas pelo menos compila e nao crasha.
console.table([1, 2]);
console.group("g");
console.groupEnd();
console.count("c");
console.time("t");
console.timeEnd("t");
console.trace();
console.clear();

print("ok");

describe("console stub methods (#310/#311)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe("ok\n")
  );
});
