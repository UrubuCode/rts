import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#303 parte 1) JS proibe \`super(...)\` mais de uma vez no mesmo
// constructor. Antes RTS chamava o construtor pai duas vezes,
// silenciosamente sobrescrevendo state. Agora rejeita em compile-time.

class Animal {
  name: string = "";
  constructor(name: string) {
    this.name = name;
  }
}

// 1. super uma vez — caso normal
class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
    super(name);
    this.breed = breed;
  }
}

const d = new Dog("Rex", "Husky");
print(`${d.name}/${d.breed}`);  // Rex/Husky

// 2. super em branch — ainda passa pq codegen so detecta seq direta
class Cat extends Animal {
  constructor(name: string) {
    if (name === "") {
      super("default");
    } else {
      super(name);
    }
  }
}
const c = new Cat("Mia");
print(c.name);  // Mia

// 3. Super seguido de outras ops — OK
class Pet extends Animal {
  age: number;
  constructor(name: string, age: number) {
    super(name);
    this.age = age;
    print(`init pet`);
  }
}
const p = new Pet("Bibi", 5);
print(`${p.name}/${p.age}`);  // Bibi/5

describe("super_call_once", () => {
  test("super legal flows OK", () =>
    expect(__rtsCapturedOutput).toBe(
      "Rex/Husky\n" +
      "Mia\n" +
      "init pet\n" +
      "Bibi/5\n"
    ));
});
