// Cross-runtime: #field in obj — ergonomic brand check (ES2022).

class Point {
  #x: number;
  #y: number;
  constructor(x: number, y: number) { this.#x = x; this.#y = y; }

  static isPoint(v: unknown): v is Point {
    return v !== null && typeof v === "object" && #x in (v as any);
  }

  distanceTo(other: unknown): number {
    if (!Point.isPoint(other)) throw new TypeError("not a Point");
    return Math.sqrt((this.#x - other.#x) ** 2 + (this.#y - other.#y) ** 2);
  }

  toString() { return "(" + this.#x + "," + this.#y + ")"; }
}

const p1 = new Point(0, 0);
const p2 = new Point(3, 4);
console.log(Point.isPoint(p1));   // true
console.log(Point.isPoint(p2));   // true
console.log(Point.isPoint({}));   // false
console.log(Point.isPoint(null)); // false
console.log(p1.distanceTo(p2));   // 5
try {
  p1.distanceTo({ x: 3, y: 4 });
} catch (e: any) {
  console.log(e.message);         // not a Point
}

// brand check across multiple classes
class Cat {
  #name: string;
  constructor(name: string) { this.#name = name; }
  static isCat(v: any) { return v !== null && typeof v === "object" && #name in v; }
  speak() { return this.#name + " says meow"; }
}

class Dog {
  #name: string;
  constructor(name: string) { this.#name = name; }
  static isDog(v: any) { return v !== null && typeof v === "object" && #name in v; }
  speak() { return this.#name + " says woof"; }
}

const cat = new Cat("Whiskers");
const dog = new Dog("Rex");

console.log(Cat.isCat(cat));   // true
console.log(Cat.isCat(dog));   // false — Dog#name is different brand
console.log(Dog.isDog(dog));   // true
console.log(Dog.isDog(cat));   // false

console.log(cat.speak());
console.log(dog.speak());
