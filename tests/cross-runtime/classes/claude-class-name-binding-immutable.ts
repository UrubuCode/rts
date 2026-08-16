// Cross-runtime: the class name is an immutable binding INSIDE the class body
// (including for a class expression, whose name is not visible outside), while
// the outer `class X` declaration binding is an ordinary mutable let.
class Declared {
  static rename(): string {
    try {
      // @ts-expect-error -- the inner class-name binding is const
      Declared = 1 as any;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static self(): boolean {
    return Declared === (this as any);
  }
  who(): string {
    return Declared.name;
  }
}

console.log("rename=" + Declared.rename());
console.log("self=" + Declared.self());
console.log("who=" + new Declared().who());

// The OUTER binding of a class declaration is mutable.
let Alias: any = Declared;
Alias = 5;
console.log("outer-rebindable=" + typeof Alias);
console.log("original-still=" + Declared.name);

// A named class expression binds its own name inside the body only.
const Expr = class Inner {
  static tryRename(): string {
    try {
      // @ts-expect-error -- the class-expression name binding is const
      Inner = null as any;
      return "no-throw";
    } catch (e: any) {
      return e.constructor.name;
    }
  }
  static selfRef(): boolean {
    return Inner === Expr;
  }
  tag(): string {
    return Inner.name;
  }
};
console.log("expr-name=" + Expr.name);
console.log("expr-rename=" + Expr.tryRename());
console.log("expr-selfref=" + Expr.selfRef());
console.log("expr-tag=" + new Expr().tag());
console.log("inner-visible-outside=" + (typeof (globalThis as any).Inner));

// The inner binding survives the outer one being reassigned.
let Holder: any = class Named {
  static resolve(): string {
    return Named.name + ":" + typeof Holder;
  }
};
const captured = Holder;
Holder = "gone";
console.log("survives=" + captured.resolve());

// An anonymous class expression takes its name from the assignment target.
const Anon = class {};
console.log("anon-name=" + Anon.name);
const obj = { Member: class {} };
console.log("member-name=" + obj.Member.name);
console.log("bare-name=" + (class {}).name);
const arr = [class {}];
console.log("array-name=" + (arr[0].name === "" ? "empty" : arr[0].name));

// name is configurable, so it can be redefined.
const nameDesc: any = Object.getOwnPropertyDescriptor(Anon, "name");
console.log("name-writable=" + nameDesc.writable);
console.log("name-configurable=" + nameDesc.configurable);
Object.defineProperty(Anon, "name", { value: "Renamed" });
console.log("renamed=" + Anon.name);

// The class binding is in TDZ while the heritage expression is evaluated.
let tdz = "unset";
try {
  class Cyclic extends (function () {
    try {
      return (Cyclic as any) === undefined ? Object : Object;
    } catch (e: any) {
      tdz = e.constructor.name;
      return Object;
    }
  })() {}
  console.log("tdz-outcome=defined");
} catch (e: any) {
  console.log("tdz-outcome=" + e.constructor.name);
}
console.log("tdz=" + tdz);

// A subclass inherits nothing of the base's name, and toString starts at the
// `class` keyword.
class BaseName {}
class SubName extends BaseName {}
console.log("sub-name=" + SubName.name);
console.log("sub-name-own=" + Object.prototype.hasOwnProperty.call(SubName, "name"));
console.log("tostring-head=" + Declared.toString().slice(0, 5));
console.log("expr-tostring-head=" + Expr.toString().slice(0, 5));
console.log("method-tostring-head=" + Declared.prototype.who.toString().slice(0, 3));
