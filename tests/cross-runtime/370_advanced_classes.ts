// Cross-runtime: advanced class features (mixins, abstract pattern, decorators via manual).

// Mixin pattern
type Constructor<T = {}> = new (...args: any[]) => T;

function Serializable<TBase extends Constructor>(Base: TBase) {
  return class extends Base {
    serialize(): string { return JSON.stringify(this); }
    static deserialize(json: string) { return JSON.parse(json); }
  };
}

function Validatable<TBase extends Constructor>(Base: TBase) {
  return class extends Base {
    validate(): boolean {
      return Object.values(this as any).every(v => v !== null && v !== undefined);
    }
  };
}

class User {
  constructor(public name: string, public age: number) {}
}
class RichUser extends Serializable(Validatable(User)) {}

const u = new RichUser("Alice", 30);
console.log(u.validate());   // true
console.log(u.serialize());  // {"name":"Alice","age":30}

const u2 = new RichUser("Bob", null as any);
console.log(u2.validate());  // false

// Manual decorator pattern (without decorator syntax — compatible with all runtimes)
function readonly(target: any, key: string, descriptor: PropertyDescriptor) {
  descriptor.writable = false;
  return descriptor;
}
function log(target: any, key: string, descriptor: PropertyDescriptor) {
  const orig = descriptor.value;
  descriptor.value = function(...args: any[]) {
    console.log("call:" + key + "(" + args.join(",") + ")");
    return orig.apply(this, args);
  };
  return descriptor;
}

class Calculator {
  add(a: number, b: number) { return a + b; }
  mul(a: number, b: number) { return a * b; }
}
// Apply manually
log(Calculator.prototype, "add", Object.getOwnPropertyDescriptor(Calculator.prototype, "add")!);
log(Calculator.prototype, "mul", Object.getOwnPropertyDescriptor(Calculator.prototype, "mul")!);

// Static factory + private constructor simulation
class Money {
  private constructor(public readonly amount: number, public readonly currency: string) {}
  static of(amount: number, currency: string) {
    if (amount < 0) throw new RangeError("negative amount");
    return new Money(amount, currency);
  }
  add(other: Money): Money {
    if (this.currency !== other.currency) throw new Error("currency mismatch");
    return Money.of(this.amount + other.amount, this.currency);
  }
  toString() { return this.amount + " " + this.currency; }
}
const a = Money.of(10, "USD");
const b = Money.of(5, "USD");
console.log(a.add(b).toString());  // 15 USD
try { Money.of(-1, "USD"); } catch (e: any) { console.log(e.message); }
try { a.add(Money.of(1, "EUR")); } catch (e: any) { console.log(e.message); }
