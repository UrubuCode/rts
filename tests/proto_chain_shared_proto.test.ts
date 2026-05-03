import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR4) Modificar Animal.prototype apos criar instances afeta
// todas as instances (proto eh compartilhado).

function Animal(): void {}
Animal.prototype.color = 7 as any;

const a1: number = new (Animal as any)();
const a2: number = new (Animal as any)();

const c1Before: number = (a1 as any).color;
const c2Before: number = (a2 as any).color;
print("c1Before=" + c1Before);
print("c2Before=" + c2Before);

// Sobrescreve no proto — afeta ambas instances
Animal.prototype.color = 99 as any;

const c1After: number = (a1 as any).color;
const c2After: number = (a2 as any).color;
print("c1After=" + c1After);
print("c2After=" + c2After);

// Adicionar nova prop no proto eh visivel em instances ja criadas
Animal.prototype.weight = 42 as any;
const w1: number = (a1 as any).weight;
const w2: number = (a2 as any).weight;
print("w1=" + w1);
print("w2=" + w2);

// Setar prop direto na instance NAO afeta o proto
collections.map_set(a1, "color", 1);  // own prop em a1 sobrescreve proto
const c1Own: number = (a1 as any).color;
const c2Still: number = (a2 as any).color;
print("c1Own=" + c1Own);
print("c2Still=" + c2Still);

describe("proto compartilhado (#264 PR4)", () => {
  test("mutacao no proto afeta instances; own prop sobrescreve", () =>
    expect(__rtsCapturedOutput).toBe(
      "c1Before=7\n" +
      "c2Before=7\n" +
      "c1After=99\n" +
      "c2After=99\n" +
      "w1=42\n" +
      "w2=42\n" +
      "c1Own=1\n" +
      "c2Still=99\n"
    ));
});
