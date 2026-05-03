import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj = { a: 1, b: 2, c: 3 };

// Object.entries — pares ordenados
const entries = Object.entries(obj);
print(`${entries.length}`);

// Object.assign
const target = { x: 1 };
Object.assign(target, { y: 2 }, { z: 3 });
print(`${(target as any).x} ${(target as any).y} ${(target as any).z}`);

// Object.freeze — v0 retorna mesmo handle (no-op)
const frozen = Object.freeze({ n: 42 });
print(`${(frozen as any).n}`);

// Object.fromEntries roundtrip
const back = Object.fromEntries(entries);
print(`${(back as any).a} ${(back as any).b} ${(back as any).c}`);

describe("object_static_methods", () => {
  test("all", () => expect(out).toBe(
    "3\n1 2 3\n42\n1 2 3\n"
  ));
});
