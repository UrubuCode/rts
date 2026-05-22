// Cross-runtime: Symbol.toPrimitive for type coercion.
class Money {
  constructor(private amount: number, private currency: string) {}
  [Symbol.toPrimitive](hint: string) {
    if (hint === "number") return this.amount;
    if (hint === "string") return this.amount + " " + this.currency;
    return this.amount;  // default
  }
}

const price = new Money(9.99, "USD");
console.log(+price);            // number hint
console.log(`${price}`);        // string hint
console.log(price + 0.01);      // default hint → number

// Symbol.toPrimitive on plain object
const counter = {
  n: 0,
  [Symbol.toPrimitive](hint: string) {
    this.n++;
    if (hint === "string") return "count:" + this.n;
    return this.n;
  }
};
console.log(+counter);
console.log(+counter);
console.log(`${counter}`);

// valueOf fallback (no Symbol.toPrimitive)
class Vector {
  constructor(public x: number, public y: number) {}
  valueOf() { return Math.sqrt(this.x * this.x + this.y * this.y); }
  toString() { return "(" + this.x + "," + this.y + ")"; }
}
const v = new Vector(3, 4);
console.log(+v);       // 5
console.log(`${v}`);   // toString
console.log(v > 4);    // valueOf comparison
