// Cross-runtime: private fields through inheritance hierarchy.

class Animal {
  #name: string;
  #energy: number;

  constructor(name: string, energy = 100) {
    this.#name = name;
    this.#energy = energy;
  }

  #consume(amount: number) {
    this.#energy = Math.max(0, this.#energy - amount);
  }

  eat(food: string, restore: number) {
    this.#energy = Math.min(100, this.#energy + restore);
    return this.#name + " eats " + food;
  }

  move(cost: number) {
    this.#consume(cost);
    return this.#name + " moves (energy:" + this.#energy + ")";
  }

  get name() { return this.#name; }
  get energy() { return this.#energy; }
}

class Dog extends Animal {
  #tricks: string[] = [];

  learn(trick: string) {
    this.#tricks.push(trick);
    return this.name + " learned " + trick;
  }

  perform() {
    if (!this.#tricks.length) return this.name + " knows no tricks";
    const trick = this.#tricks[Math.floor(this.#tricks.length / 2)];
    return this.name + " performs " + trick;
  }
}

class GuideDog extends Dog {
  #owner: string;

  constructor(name: string, owner: string) {
    super(name);
    this.#owner = owner;
  }

  guide() { return this.name + " guides " + this.#owner; }
}

const d = new Dog("Rex");
console.log(d.eat("bone", 10));     // Rex eats bone
console.log(d.move(20));            // Rex moves (energy:90)
console.log(d.learn("sit"));
console.log(d.learn("shake"));
console.log(d.perform());           // Rex performs shake (index 1)
console.log(d.energy);              // 90

const g = new GuideDog("Buddy", "Alice");
console.log(g.guide());             // Buddy guides Alice
console.log(g.eat("kibble", 5));
console.log(g.move(10));

// private fields are per-class — not cross-accessible
class A {
  #v = "A-private";
  getV() { return this.#v; }
}
class B extends A {
  #v = "B-private";  // different slot from A#v
  getBV() { return this.#v; }
}
const b = new B();
console.log(b.getV());   // A-private
console.log(b.getBV());  // B-private

// instanceof still works through private-field classes
console.log(g instanceof GuideDog);  // true
console.log(g instanceof Dog);       // true
console.log(g instanceof Animal);    // true
