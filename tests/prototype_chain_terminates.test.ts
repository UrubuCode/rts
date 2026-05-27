import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #336): iterar a prototype chain com
// `while (proto) { ...; proto = Object.getPrototypeOf(proto); }` dava uma
// VOLTA EXTRA apos "Object" — getPrototypeOf(Object.prototype) retornava o
// sentinel `[Object.prototype]` em vez de null, e o loop imprimia lixo.
// Fix: o singleton Object.prototype retorna null em getPrototypeOf (topo
// da chain).

let out = "";
function print(v: string): void { out += v + "\n"; }

class Animal { constructor(public name: string) {} }
class Dog extends Animal {}

const d = new Dog("Rex");

// constructor.name em cada nivel
print(d.constructor.name);                        // Dog
print(Object.getPrototypeOf(d).constructor.name); // Dog

// getPrototypeOf(Object.prototype) === null (topo termina)
const a = new Animal("X");
const p1 = Object.getPrototypeOf(a);   // Animal.prototype
const p2 = Object.getPrototypeOf(p1);  // Object.prototype
const p3 = Object.getPrototypeOf(p2);  // null
print("p3 null? " + (p3 === null));    // true
print("p3 falsy? " + (!p3));           // true

// iteracao completa da chain termina em Object (sem volta extra)
let proto = Object.getPrototypeOf(d);
let chain = "";
let guard = 0;
while (proto && guard < 10) {
  chain += proto.constructor.name + " ";
  proto = Object.getPrototypeOf(proto);
  guard++;
}
print(chain.trim());                   // Dog Animal Object

describe("prototype chain terminates", () => {
  test("no extra loop after Object", () =>
    expect(out).toBe("Dog\nDog\np3 null? true\np3 falsy? true\nDog Animal Object\n"));
});
