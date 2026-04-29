import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Constructor function + prototype
function Animal(name: string) {
  this.name = name;
}
Animal.prototype.speak = function() {
  return `${this.name} makes a sound`;
};
Animal.prototype.toString = function() {
  return `Animal(${this.name})`;
};

const a = new Animal("Dog");
print(`${a.speak()}`);
print(`${a.toString()}`);

// Prototype chain inheritance
function Dog(name: string, breed: string) {
  Animal.call(this, name);
  this.breed = breed;
}
Dog.prototype = Object.create(Animal.prototype);
Dog.prototype.constructor = Dog;
Dog.prototype.bark = function() {
  return `${this.name} barks`;
};

const d = new Dog("Rex", "Labrador");
print(`${d.speak()}`);
print(`${d.bark()}`);

// hasOwnProperty
print(`${d.hasOwnProperty("name")}`);
print(`${d.hasOwnProperty("speak")}`);

describe("prototype_chain", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "Dog makes a sound\nAnimal(Dog)\nRex makes a sound\nRex barks\ntrue\nfalse\n"
  ));
});
