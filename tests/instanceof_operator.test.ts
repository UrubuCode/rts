import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class Animal {
  name: string;
  constructor(name: string) { this.name = name; }
}

class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
    super(name);
    this.breed = breed;
  }
}

class Cat extends Animal {
  constructor(name: string) { super(name); }
}

const dog = new Dog("Rex", "Labrador");
const cat = new Cat("Whiskers");
const animal = new Animal("Generic");

// Direct instanceof
print(`${dog instanceof Dog}`);
print(`${dog instanceof Animal}`);
print(`${cat instanceof Cat}`);
print(`${cat instanceof Dog}`);
print(`${animal instanceof Animal}`);
print(`${animal instanceof Dog}`);

// In conditional
function describe_animal(a: Animal): void {
  if (a instanceof Dog) {
    print(`dog: ${a.name}`);
  } else if (a instanceof Cat) {
    print(`cat: ${a.name}`);
  } else {
    print(`animal: ${a.name}`);
  }
}

describe_animal(dog);
describe_animal(cat);
describe_animal(animal);

describe("instanceof_operator", () => {
  test("hierarchy", () => expect(__rtsCapturedOutput).toBe(
    "true\ntrue\ntrue\nfalse\ntrue\nfalse\ndog: Rex\ncat: Whiskers\nanimal: Generic\n"
  ));
});
