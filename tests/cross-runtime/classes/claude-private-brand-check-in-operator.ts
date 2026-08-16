// Cross-runtime: the `#x in obj` brand check answers without throwing, while a
// direct private access on a wrong-brand receiver is a TypeError. Private names
// are per-class, so a sibling class's brand never matches.
class Wallet {
  #balance: number = 10;
  #secret(): string {
    return "s" + this.#balance;
  }
  static #count: number = 0;
  static #bump(): number {
    Wallet.#count = Wallet.#count + 1;
    return Wallet.#count;
  }

  static hasBrand(o: unknown): boolean {
    return #balance in (o as any);
  }
  static hasMethodBrand(o: unknown): boolean {
    return #secret in (o as any);
  }
  static hasStaticBrand(o: unknown): boolean {
    return #count in (o as any);
  }
  static bump(): number {
    return Wallet.#bump();
  }

  // Reading another instance's private of the same class is allowed.
  peek(other: Wallet): number {
    return other.#balance;
  }
  read(): number {
    return this.#balance;
  }
  callSecret(): string {
    return this.#secret();
  }
  raw(o: any): number {
    return o.#balance;
  }
}

class Decoy {
  #balance: number = 99;
  read(): number {
    return this.#balance;
  }
}

const a = new Wallet();
const b = new Wallet();
const d = new Decoy();

console.log("a-read=" + a.read());
console.log("a-peek-b=" + a.peek(b));
console.log("a-secret=" + a.callSecret());

console.log("brand-a=" + Wallet.hasBrand(a));
console.log("brand-b=" + Wallet.hasBrand(b));
console.log("brand-decoy=" + Wallet.hasBrand(d));
console.log("brand-plain=" + Wallet.hasBrand({ balance: 1 }));
console.log("brand-proto=" + Wallet.hasBrand(Wallet.prototype));
console.log("brand-null-proto=" + Wallet.hasBrand(Object.create(null)));

console.log("method-brand-a=" + Wallet.hasMethodBrand(a));
console.log("method-brand-proto=" + Wallet.hasMethodBrand(Wallet.prototype));

console.log("static-brand-ctor=" + Wallet.hasStaticBrand(Wallet));
console.log("static-brand-inst=" + Wallet.hasStaticBrand(a));

console.log("bump1=" + Wallet.bump());
console.log("bump2=" + Wallet.bump());

// Wrong brand through a direct access: TypeError, not undefined.
try {
  a.raw(d);
  console.log("wrong-brand=no-throw");
} catch (e: any) {
  console.log("wrong-brand=" + e.constructor.name);
  console.log("wrong-brand-is-type=" + (e instanceof TypeError));
}

try {
  a.raw({ balance: 5 });
  console.log("plain=no-throw");
} catch (e: any) {
  console.log("plain=" + e.constructor.name);
}

try {
  a.raw(Wallet.prototype);
  console.log("proto-access=no-throw");
} catch (e: any) {
  console.log("proto-access=" + e.constructor.name);
}

// Privates never appear in any reflection surface.
console.log("own-names=" + Object.getOwnPropertyNames(a).length);
console.log("own-symbols=" + Object.getOwnPropertySymbols(a).length);
console.log("keys=" + Object.keys(a).length);
console.log("json=" + JSON.stringify(a));
console.log("decoy-read=" + d.read());

// Brand survives a prototype swap: the slot lives on the instance.
const swapped: any = new Wallet();
Object.setPrototypeOf(swapped, Decoy.prototype);
console.log("swapped-brand=" + Wallet.hasBrand(swapped));
console.log("swapped-instanceof=" + (swapped instanceof Wallet));
try {
  console.log("swapped-decoy-read=" + swapped.read());
} catch (e: any) {
  console.log("swapped-decoy-read=" + e.constructor.name);
}
