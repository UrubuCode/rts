// Cross-runtime: private accessors, a private method's un-writability, and the
// static private brand — which lives on the declaring constructor only, so an
// inherited static method reaching a private static of a SUBCLASS throws.
class Counter {
  #n: number = 0;
  #step: number = 1;

  get #value(): number {
    return this.#n;
  }
  set #value(v: number) {
    this.#n = v < 0 ? 0 : v;
  }
  #advance(): number {
    this.#value = this.#value + this.#step;
    return this.#value;
  }

  bump(): number {
    return this.#advance();
  }
  set(v: number): number {
    this.#value = v;
    return this.#value;
  }
  read(): number {
    return this.#value;
  }
  // A private method is not writable: assigning to it is a TypeError.
  tryWriteMethod(): string {
    try {
      // @ts-expect-error -- deliberately assigning to a private method
      this.#advance = 1;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  // Reading a getter-only private through a class that only declared the pair
  // is fine; here the pair is complete, so both directions work.
  static probe(o: any): string {
    if (#n in o) return "branded";
    return "unbranded";
  }
}

const c = new Counter();
console.log("read0=" + c.read());
console.log("bump1=" + c.bump());
console.log("bump2=" + c.bump());
console.log("set-neg=" + c.set(-5));
console.log("set-pos=" + c.set(7));
console.log("read-final=" + c.read());
console.log("write-method=" + c.tryWriteMethod());
console.log("probe-inst=" + Counter.probe(c));
console.log("probe-other=" + Counter.probe({}));
console.log("own-keys=" + Object.getOwnPropertyNames(c).join(","));

// A getter-only private: writing it is a TypeError even from inside the class.
class ReadOnlyPriv {
  #store: number = 3;
  get #view(): number {
    return this.#store;
  }
  read(): number {
    return this.#view;
  }
  write(): string {
    try {
      // @ts-expect-error -- deliberately assigning a getter-only private
      this.#view = 9;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
}
const rp = new ReadOnlyPriv();
console.log("ro-read=" + rp.read());
console.log("ro-write=" + rp.write());

// The static private brand is on the constructor object that declared it.
class StaticHolder {
  static #total: number = 100;
  static #tick(): number {
    StaticHolder.#total = StaticHolder.#total + 1;
    return StaticHolder.#total;
  }
  static get #half(): number {
    return StaticHolder.#total / 2;
  }

  static tick(): number {
    return StaticHolder.#tick();
  }
  static half(): number {
    return StaticHolder.#half;
  }
  // `this` here is the receiver, which for a subclass call is the subclass.
  static tickOnThis(this: any): string {
    try {
      this.#total = this.#total + 1;
      return "ok:" + this.#total;
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static hasBrand(o: any): boolean {
    return #total in o;
  }
}

class SubHolder extends StaticHolder {}

console.log("tick1=" + StaticHolder.tick());
console.log("tick2=" + StaticHolder.tick());
console.log("half=" + StaticHolder.half());
console.log("brand-base=" + StaticHolder.hasBrand(StaticHolder));
console.log("brand-sub=" + StaticHolder.hasBrand(SubHolder));
console.log("brand-proto-of-sub=" + (Object.getPrototypeOf(SubHolder) === StaticHolder));
console.log("this-on-base=" + StaticHolder.tickOnThis());
console.log("this-on-sub=" + (SubHolder as any).tickOnThis());
console.log("sub-tick=" + SubHolder.tick());
console.log("total-after=" + StaticHolder.half() * 2);

// Private names are lexical: an inner class sees the outer #n as well as its
// own #m, and each brand check answers only for its declaring class.
class Outer {
  #n: string = "outer";
  makeInner(): any {
    const self = this;
    class Inner {
      #m: string = "inner";
      both(): string {
        return this.#m + "/" + self.#n;
      }
      static innerBrand(o: any): boolean {
        return #m in o;
      }
      static outerBrandFromInner(o: any): boolean {
        return #n in o;
      }
    }
    return Inner;
  }
  static outerBrand(o: any): boolean {
    return #n in o;
  }
}
const o = new Outer();
const Inner: any = o.makeInner();
const inner = new Inner();
console.log("nested=" + inner.both());
console.log("inner-brand-inner=" + Inner.innerBrand(inner));
console.log("inner-brand-outer=" + Inner.innerBrand(o));
console.log("outer-from-inner-inner=" + Inner.outerBrandFromInner(inner));
console.log("outer-from-inner-outer=" + Inner.outerBrandFromInner(o));
console.log("outer-brand-inner=" + Outer.outerBrand(inner));
console.log("outer-brand-outer=" + Outer.outerBrand(o));
