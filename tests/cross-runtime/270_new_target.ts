// Cross-runtime: new.target in functions and constructors.
function F(this: any) { return new.target ? "new" : "call"; }
function wrap() { const arrow = () => String(new.target ? "new" : "undef"); return arrow(); }
class Base { kind = String(new.target?.name); }
class Sub extends Base { sub = String(new.target?.name); }
console.log("f1=" + F.call({}));
console.log("f2=" + new (F as any)());
console.log("wrap=" + wrap());
const s = new Sub();
console.log("cls=" + s.kind + ":" + s.sub);
