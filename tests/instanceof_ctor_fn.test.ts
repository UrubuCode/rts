import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #387): `instance instanceof CtorFn` para
// FUNCAO-CONSTRUTORA (pre-ES6) dava `error: instanceof RHS is not a known
// class` (a fn nao esta em ctx.classes nem global classes). Fix: reifica a
// fn, resolve seu prototype e anda a __proto__ chain da instancia via
// INSTANCEOF_PROTO. Cobre heranca por Object.create(Base.prototype).

let out = "";
function print(v: string): void { out += v + "\n"; }

function Animal(name: string) { this.name = name; }
Animal.prototype.speak = function () { return this.name + " som"; };

function Dog(name: string) { Animal.call(this, name); }
Dog.prototype = Object.create(Animal.prototype);
Dog.prototype.constructor = Dog;
Dog.prototype.bark = function () { return this.name + " late"; };

const a = new Animal("Mimi");
const d = new Dog("Rex");

// instanceof direto
print("" + (a instanceof Animal));   // true
print("" + (d instanceof Dog));      // true
// instanceof herdado via prototype chain
print("" + (d instanceof Animal));   // true
// negativo: Animal NAO eh instancia de Dog
print("" + (a instanceof Dog));      // false
// metodos herdados continuam funcionando (regressao de chain)
print(d.speak());                    // Rex som
print(d.bark());                     // Rex late

// primitivo nunca casa
const n = 42;
print("" + ((n as any) instanceof Animal)); // false

describe("instanceof ctor fn", () => {
  test("chain", () =>
    expect(out).toBe("true\ntrue\ntrue\nfalse\nRex som\nRex late\nfalse\n"));
});
