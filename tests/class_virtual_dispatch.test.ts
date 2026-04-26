import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class Animal {
  speak(): string { return "generic"; }
}
class Cat extends Animal {
  speak(): string { return "meow"; }
}
class Dog extends Animal {
  speak(): string { return "woof"; }
}

function makeNoise(a: Animal): void { print(a.speak()); }
makeNoise(new Cat());
makeNoise(new Dog());
makeNoise(new Animal());

class Shape { area(): i32 { return 0; } }
class Square extends Shape {
  s: i32;
  constructor(s: i32) { super(); this.s = s; }
  area(): i32 { return this.s * this.s; }
}
class Circle extends Shape {
  r: i32;
  constructor(r: i32) { super(); this.r = r; }
  area(): i32 { return this.r * this.r * 3; }
}

const shapes: Shape[] = [new Square(4), new Circle(3), new Square(2)];
let totalArea: i32 = 0;
for (const sh of shapes) {
  totalArea += sh.area();
}
print(`total area=${totalArea}`);

class Base {
  describe(): string { return "base"; }
  greet(): void { print(`Hi from ${this.describe()}`); }
}
class Derived extends Base {
  describe(): string { return "derived"; }
}
const der: Derived = new Derived();
der.greet();
const ba: Base = new Base();
ba.greet();

describe("fixture:class_virtual_dispatch", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("meow\nwoof\ngeneric\ntotal area=47\nHi from derived\nHi from base\n");
  });
});
