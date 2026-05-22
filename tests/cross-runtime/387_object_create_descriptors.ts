// Cross-runtime: Object.create with full descriptors + prototype manipulation.

// Object.create(proto, descriptors)
const animal = {
  breathe() { return this.name + " breathes"; }
};
const dog = Object.create(animal, {
  name:  { value: "Rex", writable: true, enumerable: true, configurable: true },
  breed: { value: "Lab", writable: false, enumerable: true, configurable: false },
  _age:  { value: 3, writable: true, enumerable: false, configurable: true },
});
console.log(dog.breathe());         // Rex breathes
console.log(dog.name);              // Rex
console.log(dog.breed);             // Lab
console.log(Object.keys(dog).join(",")); // name,breed (_age not enumerable)
console.log(Object.getPrototypeOf(dog) === animal); // true

// Object.create(null) — no prototype
const dict = Object.create(null) as Record<string, number>;
dict.a = 1; dict.b = 2;
console.log("toString" in dict);  // false — no Object.prototype
console.log(Object.keys(dict).join(","));  // a,b

// Object.setPrototypeOf (dynamic prototype swap)
const base1 = { greet() { return "hello from base1"; } };
const base2 = { greet() { return "hello from base2"; } };
const obj: any = Object.create(base1);
console.log(obj.greet());  // hello from base1
Object.setPrototypeOf(obj, base2);
console.log(obj.greet());  // hello from base2

// Prototype chain inspection
class A { methodA() { return "A"; } }
class B extends A { methodB() { return "B"; } }
class C extends B { methodC() { return "C"; } }
const c = new C();
const chain: string[] = [];
let proto = Object.getPrototypeOf(c);
while (proto) {
  if (proto.constructor?.name) chain.push(proto.constructor.name);
  proto = Object.getPrototypeOf(proto);
}
console.log(chain.join("->"));  // C->B->A->Object

// Object.create for classical inheritance without class keyword
function Shape(this: any, color: string) { this.color = color; }
Shape.prototype.describe = function() { return this.color + " shape"; };

function Circle(this: any, color: string, r: number) {
  Shape.call(this, color);
  this.radius = r;
}
Circle.prototype = Object.create(Shape.prototype, {
  constructor: { value: Circle, writable: true, configurable: true },
  area: { value: function(this: any) { return Math.PI * this.radius ** 2; }, writable: true, configurable: true }
});
const circ = new (Circle as any)("red", 5);
console.log(circ.describe());                      // red shape
console.log(Math.round(circ.area()));              // 79
console.log(circ instanceof Circle);               // true
console.log(circ instanceof (Shape as any));       // true
