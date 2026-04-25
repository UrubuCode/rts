import { io } from "rts";

class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  describe(): void {
    io.print(`I am ${this.name}`);
  }
}

class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
    super(name);
    this.breed = breed;
  }
  bark(): void {
    io.print(`Woof! I am a ${this.breed}`);
  }
}

class Puppy extends Dog {
  age: i32;
  constructor(name: string, breed: string, age: i32) {
    super(name, breed);
    this.age = age;
  }
  introduce(): void {
    io.print(`I am ${this.name}, a ${this.age}yo ${this.breed}`);
  }
}

const d: Dog = new Dog("Rex", "Husky");
d.describe();
d.bark();

const p: Puppy = new Puppy("Buddy", "Lab", 1);
p.describe();
p.bark();
p.introduce();
