// Cross-runtime: new.target inside a base reached through super() names the
// most-derived constructor, Reflect.construct can hand it an unrelated one,
// and it is undefined in a plain call.
const seen: string[] = [];

class Base {
  which: string;
  constructor() {
    const nt: any = new.target;
    seen.push(nt === undefined ? "undefined" : nt.name);
    this.which = nt === undefined ? "undefined" : nt.name;
  }
}

class Mid extends Base {}
class Leaf extends Mid {}

console.log("base=" + new Base().which);
console.log("mid=" + new Mid().which);
console.log("leaf=" + new Leaf().which);

// Reflect.construct picks newTarget explicitly; the prototype comes from it.
const viaReflect: any = Reflect.construct(Base, [], Leaf);
console.log("reflect-which=" + viaReflect.which);
console.log("reflect-proto=" + (Object.getPrototypeOf(viaReflect) === Leaf.prototype));
console.log("reflect-instanceof-leaf=" + (viaReflect instanceof Leaf));
console.log("reflect-instanceof-base=" + (viaReflect instanceof Base));

// With no explicit newTarget, newTarget is the target itself.
const plainReflect: any = Reflect.construct(Base, []);
console.log("reflect-default=" + plainReflect.which);

function Fn(this: any) {
  this.nt = new.target === undefined ? "undefined" : "Fn";
  return undefined;
}
console.log("fn-new=" + (new (Fn as any)()).nt);
const bare: any = {};
(Fn as any).call(bare);
console.log("fn-call=" + bare.nt);

// new.target in an arrow is the enclosing function's, not the arrow's.
class ArrowHolder {
  probe: string;
  constructor() {
    const f = () => (new.target === undefined ? "undefined" : (new.target as any).name);
    this.probe = f();
  }
}
console.log("arrow=" + new ArrowHolder().probe);

class ArrowSub extends ArrowHolder {}
console.log("arrow-sub=" + new ArrowSub().probe);

// new.target in a method body invoked normally is undefined.
const holder = {
  m() {
    return new.target === undefined ? "undefined" : "set";
  },
};
console.log("method=" + holder.m());

// A newTarget that is not the class still runs the class's field initialisers.
class WithField extends Base {
  marker: string = "field";
}
const odd: any = Reflect.construct(WithField, [], Leaf);
console.log("odd-which=" + odd.which);
console.log("odd-marker=" + odd.marker);
console.log("odd-proto=" + (Object.getPrototypeOf(odd) === Leaf.prototype));
console.log("odd-keys=" + Object.keys(odd).join(","));

// Reflect.construct with a newTarget whose prototype is not an object falls
// back to the intrinsic Object.prototype.
function Weird() {}
(Weird as any).prototype = 7;
const fallback: any = Reflect.construct(Base, [], Weird as any);
console.log("fallback-proto=" + (Object.getPrototypeOf(fallback) === Object.prototype));
console.log("fallback-which=" + fallback.which);

console.log("seen=" + seen.join("|"));
console.log("seen-len=" + seen.length);
