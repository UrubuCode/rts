// Cross-runtime: getters and setters on objects and classes.

// object literal getter/setter
const obj = {
  _v: 0,
  get value() { return this._v; },
  set value(n: number) { this._v = n < 0 ? 0 : n; }
};
console.log(obj.value);
obj.value = 5;
console.log(obj.value);
obj.value = -3;
console.log(obj.value);  // clamped to 0

// class getter/setter
class Temperature {
  private _c: number;
  constructor(c: number) { this._c = c; }
  get celsius() { return this._c; }
  set celsius(v: number) { this._c = v; }
  get fahrenheit() { return this._c * 9 / 5 + 32; }
  set fahrenheit(v: number) { this._c = (v - 32) * 5 / 9; }
}
const t = new Temperature(0);
console.log(t.celsius);
console.log(t.fahrenheit);
t.fahrenheit = 212;
console.log(t.celsius);

// static getter
class Config {
  static get version() { return "1.0.0"; }
}
console.log(Config.version);

// Object.defineProperty getter
const o: any = {};
let _secret = 0;
Object.defineProperty(o, "secret", {
  get() { return _secret; },
  set(v: number) { _secret = v * 2; }
});
o.secret = 5;
console.log(o.secret);  // 10
