// Cross-runtime: a class field is installed with [[DefineOwnProperty]], not
// [[Set]] — so it shadows an inherited setter instead of calling it, and it
// overwrites a non-writable inherited property without a TypeError.
const calls: string[] = [];

class Base {
  get shadowed(): string {
    return "base-getter";
  }
  set shadowed(v: string) {
    calls.push("setter:" + v);
  }
}

class DerivedField extends Base {
  shadowed: string = "field-value";
}

class DerivedAssign extends Base {
  constructor() {
    super();
    // An ordinary assignment DOES find the inherited setter.
    (this as any).shadowed = "assigned";
  }
}

const df: any = new DerivedField();
console.log("field-value=" + df.shadowed);
console.log("field-own=" + Object.prototype.hasOwnProperty.call(df, "shadowed"));
console.log("field-calls=" + calls.join("|"));
const fd: any = Object.getOwnPropertyDescriptor(df, "shadowed");
console.log("field-desc=w" + fd.writable + ",e" + fd.enumerable + ",c" + fd.configurable);

calls.length = 0;
const da: any = new DerivedAssign();
console.log("assign-value=" + da.shadowed);
console.log("assign-own=" + Object.prototype.hasOwnProperty.call(da, "shadowed"));
console.log("assign-calls=" + calls.join("|"));

// A non-writable inherited data property: the field defines over it silently.
const frozenProto: any = {};
Object.defineProperty(frozenProto, "locked", { value: "proto", writable: false, enumerable: false, configurable: false });

class OverLocked {
  locked: string = "field";
}
Object.setPrototypeOf(OverLocked.prototype, frozenProto);
const ol: any = new OverLocked();
console.log("locked-value=" + ol.locked);
console.log("locked-own=" + Object.prototype.hasOwnProperty.call(ol, "locked"));

class AssignLocked {
  constructor() {
    try {
      (this as any).locked = "assigned";
      (this as any).result = "no-throw";
    } catch (e: any) {
      (this as any).result = e.constructor.name;
    }
  }
}
Object.setPrototypeOf(AssignLocked.prototype, frozenProto);
const al: any = new AssignLocked();
console.log("locked-assign=" + al.result);
console.log("locked-assign-value=" + al.locked);

// A field with no initialiser still defines the property, as undefined.
class Undef {
  a: any = undefined;
  b: any = 1;
}
const u: any = new Undef();
console.log("undef-keys=" + Object.keys(u).join(","));
console.log("undef-a=" + String(u.a));
console.log("undef-has-a=" + ("a" in u));
console.log("undef-json=" + JSON.stringify(u));

// A static field defines on the constructor, shadowing an inherited static
// accessor the same way.
class SBase {
  static get s(): string {
    return "sbase";
  }
  static set s(v: string) {
    calls.push("static-setter:" + v);
  }
}
calls.length = 0;
class SDerived extends SBase {
  static s: string = "sfield";
}
console.log("static-field=" + SDerived.s);
console.log("static-base=" + SBase.s);
console.log("static-calls=" + calls.join("|"));
console.log("static-own=" + Object.prototype.hasOwnProperty.call(SDerived, "s"));

// A field named "constructor" or "prototype" is legal on an instance.
class OddNames {
  constructorTag: string = "ct";
  prototype: string = "p";
}
const on: any = new OddNames();
console.log("odd-keys=" + Object.keys(on).join(","));
console.log("odd-prototype=" + on.prototype);
console.log("odd-ctor=" + on.constructor.name);

// Field installation happens before the derived constructor body, and each
// instance gets its own copy of an object-valued field.
class Fresh {
  bag: number[] = [];
}
const f1 = new Fresh();
const f2 = new Fresh();
f1.bag.push(1);
console.log("fresh-1=" + f1.bag.length);
console.log("fresh-2=" + f2.bag.length);
console.log("fresh-shared=" + (f1.bag === f2.bag));
