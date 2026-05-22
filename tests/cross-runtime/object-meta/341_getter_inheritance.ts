// Cross-runtime: getter/setter inheritance and override in class hierarchy.
class Base {
  protected _x = 0;
  get x() { return this._x; }
  set x(v: number) { this._x = v; }
  describe() { return "base:" + this._x; }
}

class Child extends Base {
  get x() { return this._x * 2; }  // override getter only
  set x(v: number) { this._x = v; }
}

const b = new Base();
b.x = 5;
console.log(b.x);        // 5
console.log(b.describe());

const c = new Child();
c.x = 5;
console.log(c.x);        // 10 (doubled)
console.log(c.describe()); // base:5

// accessor via Object.defineProperty on prototype
class Widget {
  private _visible = true;
}
Object.defineProperty(Widget.prototype, "visible", {
  get(this: any) { return this._visible; },
  set(this: any, v: boolean) { this._visible = v; }
});
const w: any = new Widget();
console.log(w.visible);
w.visible = false;
console.log(w.visible);
