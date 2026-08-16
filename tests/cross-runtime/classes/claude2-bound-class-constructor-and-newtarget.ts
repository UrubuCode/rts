// Cross-runtime: a class constructor put through Function.prototype.bind. The
// bound function constructs, but new.target inside names the ORIGINAL class,
// the bound arguments prepend, and the bound function has no `prototype` — so
// it must be put back before the bound function can be extended.
const seen: string[] = [];

class Point {
  x: number = 0;
  y: number = 0;
  target: string = "";
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
    this.target = new.target === undefined ? "none" : (new.target as any).name;
    seen.push("Point(" + x + "," + y + ")");
  }
  sum(): number {
    return this.x + this.y;
  }
}

const BoundPoint: any = Point.bind(null, 10);
console.log("bound-name=" + BoundPoint.name);
console.log("bound-length=" + BoundPoint.length);
console.log("bound-has-prototype=" + Object.prototype.hasOwnProperty.call(BoundPoint, "prototype"));
console.log("bound-prototype=" + String(BoundPoint.prototype));
console.log("bound-typeof=" + typeof BoundPoint);

const bp = new BoundPoint(5);
console.log("bp-x=" + bp.x + ",y=" + bp.y);
console.log("bp-sum=" + bp.sum());
console.log("bp-newtarget=" + bp.target);
console.log("bp-instanceof-point=" + (bp instanceof Point));
console.log("bp-instanceof-bound=" + (bp instanceof BoundPoint));
console.log("bp-proto-is-point=" + (Object.getPrototypeOf(bp) === Point.prototype));

// Binding twice keeps prepending, and new.target still resolves to Point.
const Bound2: any = BoundPoint.bind(null, 7);
const bp2 = new Bound2();
console.log("bound2-name=" + Bound2.name);
console.log("bound2-length=" + Bound2.length);
console.log("bp2-x=" + bp2.x + ",y=" + bp2.y);
console.log("bp2-newtarget=" + bp2.target);

// A bound class still refuses a plain call.
let plainCall = "no-throw";
try {
  BoundPoint(1);
} catch (e: any) {
  plainCall = e.constructor.name;
}
console.log("bound-plain-call=" + plainCall);

// Reflect.construct through the bound function, with and without an explicit
// newTarget. An explicit one is NOT replaced by the bind target.
const rc: any = Reflect.construct(BoundPoint, [1]);
console.log("rc-x=" + rc.x + ",y=" + rc.y + ",target=" + rc.target);

class Other {
  static marker: string = "other";
}
const rc2: any = Reflect.construct(BoundPoint, [2], Other);
console.log("rc2-newtarget=" + rc2.target);
console.log("rc2-proto-is-other=" + (Object.getPrototypeOf(rc2) === Other.prototype));
console.log("rc2-instanceof-point=" + (rc2 instanceof Point));

// A subclass reached through super() reports the most-derived class, and a
// BOUND subclass reports the unbound subclass.
class Point3 extends Point {
  z: number = 0;
  constructor(x: number, y: number, z: number) {
    super(x, y);
    this.z = z;
  }
}
const p3 = new Point3(1, 2, 3);
console.log("p3-newtarget=" + p3.target);
const BoundP3: any = Point3.bind(null, 4, 5);
const bp3 = new BoundP3(6);
console.log("bp3-newtarget=" + bp3.target);
console.log("bp3-z=" + bp3.z);
console.log("bp3-instanceof=" + (bp3 instanceof Point3) + "," + (bp3 instanceof Point));

// `extends` reads `prototype` off the heritage value, and a bound function has
// none — so a bound constructor becomes extendable once a prototype is put
// back on it, and the chain it produces is the original class's.
const Restored: any = Point.bind(null);
Object.defineProperty(Restored, "prototype", { value: Point.prototype, writable: false });
class FromRestored extends Restored {
  tag: string = "restored";
}
const fr: any = new FromRestored(8, 9);
console.log("restored-x=" + fr.x + ",y=" + fr.y);
console.log("restored-tag=" + fr.tag);
console.log("restored-newtarget=" + fr.target);
console.log("restored-instanceof=" + (fr instanceof FromRestored) + "," + (fr instanceof Point));

console.log("seen=" + seen.join("|"));
console.log("seen-len=" + seen.length);
