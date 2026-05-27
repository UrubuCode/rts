import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// `x instanceof (C as any)` / `x instanceof (C)` — o RHS com cast TS ou
// parenteses deve resolver para o identificador da classe/construtora.
// Antes o codegen rejeitava com "RHS must be a class identifier".
class Animal { name = "a"; }
class Dog extends Animal { breed = "b"; }
function Shape(this: any) {}

const d = new Dog();
print("dog_as_animal=" + (d instanceof (Animal as any)));
print("dog_paren=" + (d instanceof (Dog)));

const s = new (Shape as any)();
print("shape_cast=" + (s instanceof (Shape as any)));

describe("instanceof RHS com cast/paren (387)", () => {
  test("peel de TS cast e paren no RHS", () =>
    expect(out).toBe("dog_as_animal=true\ndog_paren=true\nshape_cast=true\n"));
});
