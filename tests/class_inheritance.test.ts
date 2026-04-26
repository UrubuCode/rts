import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  describe(): void {
    print(`I am ${this.name}`);
  }
}

class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
    super(name);
    this.breed = breed;
  }
  bark(): void {
    print(`Woof! I am a ${this.breed}`);
  }
}

class Puppy extends Dog {
  age: i32;
  constructor(name: string, breed: string, age: i32) {
    super(name, breed);
    this.age = age;
  }
  introduce(): void {
    print(`I am ${this.name}, a ${this.age}yo ${this.breed}`);
  }
}

const d: Dog = new Dog("Rex", "Husky");
d.describe();
d.bark();

const p: Puppy = new Puppy("Buddy", "Lab", 1);
p.describe();
p.bark();
p.introduce();

describe("fixture:class_inheritance", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("I am Rex\nWoof! I am a Husky\nI am Buddy\nWoof! I am a Lab\nI am Buddy, a 1yo Lab\n");
  });
});
