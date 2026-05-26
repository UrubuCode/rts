import { describe, test, expect } from "rts:test";

// Regression (375): private fields através de herança + default params em
// constructor (próprio, herdado e via super) + getter/método retornando
// campo string herdam o tipo correto (não handle cru).

class Animal {
  #name: string;
  #energy: number;
  constructor(name: string, energy = 100) {
    this.#name = name;
    this.#energy = energy;
  }
  #consume(amount: number) { this.#energy = Math.max(0, this.#energy - amount); }
  eat(food: string, restore: number) {
    this.#energy = Math.min(100, this.#energy + restore);
    return this.#name + " eats " + food;
  }
  move(cost: number) { this.#consume(cost); return this.#name + " moves:" + this.#energy; }
  get name() { return this.#name; }
  get energy() { return this.#energy; }
}
class Dog extends Animal {
  #tricks: string[] = [];
  learn(trick: string) { this.#tricks.push(trick); return this.name + " learned " + trick; }
}
class GuideDog extends Dog {
  #owner: string;
  constructor(name: string, owner: string) { super(name); this.#owner = owner; }
  guide() { return this.name + " guides " + this.#owner; }
}

// Dog herda constructor(name, energy=100) de Animal — default aplicado.
const d = new Dog("Rex");
const dEat = d.eat("bone", 10);   // 100
const dMove = d.move(20);         // 80
const dLearn = d.learn("sit");
const dEnergy = d.energy;         // 80

// GuideDog: super(name) aplica default energy=100 de Animal.
const g = new GuideDog("Buddy", "Alice");
const gGuide = g.guide();
g.eat("kibble", 5);               // 100
const gMove = g.move(10);         // 90

// private com mesmo nome em hierarquia — slots distintos.
class A { #v = "A-priv"; getV() { return this.#v; } }
class B extends A { #v = "B-priv"; getBV() { return this.#v; } }
const b = new B();

describe("private inheritance + defaults", () => {
  test("getter string herdado retorna nome", () => expect(dEat).toBe("Rex eats bone"));
  test("default energy=100 herdado, move atualiza", () => expect(dMove).toBe("Rex moves:80"));
  test("getter name via this em método de subclasse", () => expect(dLearn).toBe("Rex learned sit"));
  test("energy persiste através de método privado", () => expect(dEnergy).toBe(80));
  test("super(name) aplica default energy", () => expect(gMove).toBe("Buddy moves:90"));
  test("guide usa getter herdado + private próprio", () => expect(gGuide).toBe("Buddy guides Alice"));
  test("private #v slot da base", () => expect(b.getV()).toBe("A-priv"));
  test("private #v slot da subclasse", () => expect(b.getBV()).toBe("B-priv"));
});
