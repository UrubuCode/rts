// Cross-runtime: overridden method dispatched polymorphically while sweeping
// an ARRAY of instances of the same base class.
class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  speak(): string {
    return "...";
  }
  intro(): string {
    return this.name + " says " + this.speak();
  }
  legs(): number {
    return 4;
  }
}

class Dog extends Animal {
  speak(): string {
    return "woof";
  }
}

class Cat extends Animal {
  speak(): string {
    return "meow";
  }
}

class Bird extends Animal {
  speak(): string {
    return "tweet";
  }
  legs(): number {
    return 2;
  }
}

class Puppy extends Dog {
  speak(): string {
    return super.speak() + "!";
  }
}

const zoo: Animal[] = [
  new Dog("rex"),
  new Cat("tom"),
  new Bird("kiwi"),
  new Puppy("bud"),
  new Animal("blob"),
];

// indexed access + method call in a classic for loop
for (let i = 0; i < zoo.length; i++) {
  console.log("idx" + i + "=" + zoo[i].speak());
}

// indexed access calling a method that calls another virtual method
for (let i = 0; i < zoo.length; i++) {
  console.log("intro" + i + "=" + zoo[i].intro());
}

// for..of over the same array
const viaOf: string[] = [];
for (const a of zoo) {
  viaOf.push(a.speak());
}
console.log("for_of=" + viaOf.join(","));

// map with a method reference through the element
console.log("map=" + zoo.map((a) => a.speak()).join(","));
console.log("map_legs=" + zoo.map((a) => a.legs()).join(","));

// sum through a virtual method
let total = 0;
for (let i = 0; i < zoo.length; i++) {
  total += zoo[i].legs();
}
console.log("total_legs=" + total);

// filter + chained dispatch
console.log("four=" + zoo.filter((a) => a.legs() === 4).map((a) => a.name).join(","));

// nested array of instances
const nested: Animal[][] = [[zoo[0], zoo[1]], [zoo[2]], [zoo[3], zoo[4]]];
const flat: string[] = [];
for (let i = 0; i < nested.length; i++) {
  for (let j = 0; j < nested[i].length; j++) {
    flat.push(nested[i][j].speak());
  }
}
console.log("nested=" + flat.join(","));

// re-reading the same slot twice, and after mutation
console.log("slot0_twice=" + zoo[0].speak() + "/" + zoo[0].speak());
zoo[0] = new Cat("swap");
console.log("after_swap=" + zoo[0].speak() + ":" + zoo[0].name);

// element pulled out into a local first
const picked = zoo[3];
console.log("picked=" + picked.speak() + ":" + picked.intro());
