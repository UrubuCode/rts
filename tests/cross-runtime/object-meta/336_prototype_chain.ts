// Cross-runtime: prototype chain manipulation.
function Animal(this: any, name: string) { this.name = name; }
Animal.prototype.speak = function() { return this.name + " speaks"; };

function Dog(this: any, name: string) {
  Animal.call(this, name);
}
Dog.prototype = Object.create(Animal.prototype);
Dog.prototype.constructor = Dog;
Dog.prototype.bark = function() { return this.name + " barks"; };

const d = new (Dog as any)("Rex");
console.log(d.speak());
console.log(d.bark());
console.log(d instanceof Dog);
console.log(d instanceof Animal);
console.log(d.constructor === Dog);

// hasOwnProperty vs prototype
console.log(d.hasOwnProperty("name"));
console.log(d.hasOwnProperty("speak"));

// Object.getPrototypeOf
console.log(Object.getPrototypeOf(d) === Dog.prototype);
console.log(Object.getPrototypeOf(Dog.prototype) === Animal.prototype);
