// Cross-runtime: get/set accessor pair with the same name over a backing field.
// Focus: read-through, write-through, clamping, inheritance and override.
class Temp {
  _c: number;
  constructor(c: number) {
    this._c = c;
  }
  get celsius(): number {
    return this._c;
  }
  set celsius(v: number) {
    this._c = v < -273 ? -273 : v;
  }
  get fahrenheit(): number {
    return this._c * 9 / 5 + 32;
  }
  set fahrenheit(v: number) {
    this.celsius = (v - 32) * 5 / 9;
  }
}

const t = new Temp(0);
console.log("c0=" + t.celsius);
console.log("f0=" + t.fahrenheit);
t.celsius = 100;
console.log("c1=" + t.celsius);
console.log("f1=" + t.fahrenheit);
t.fahrenheit = 32;
console.log("c2=" + t.celsius);
console.log("backing2=" + t._c);
t.celsius = -1000;
console.log("clamped=" + t.celsius);

// setter delegating into another setter
t.fahrenheit = -500;
console.log("clamped_via_f=" + t.celsius);

class ReadOnly {
  _v: number;
  constructor(v: number) {
    this._v = v;
  }
  get v(): number {
    return this._v;
  }
}
const r = new ReadOnly(7);
console.log("ro_get=" + r.v);

// accessor inherited by subclass
class Kelvin extends Temp {
  get kelvin(): number {
    return this.celsius + 273;
  }
  set kelvin(v: number) {
    this.celsius = v - 273;
  }
}
const k = new Kelvin(10);
console.log("k_celsius=" + k.celsius);
console.log("k_kelvin=" + k.kelvin);
k.kelvin = 300;
console.log("k_after=" + k.celsius);
console.log("k_f_inherited=" + k.fahrenheit);

// subclass overriding the inherited accessor pair
class Doubled extends Temp {
  get celsius(): number {
    return this._c * 2;
  }
  set celsius(v: number) {
    this._c = v;
  }
}
const d = new Doubled(5);
console.log("d_get=" + d.celsius);
d.celsius = 8;
console.log("d_after=" + d.celsius);
console.log("d_backing=" + d._c);

// accessors live on the prototype, backing field on the instance
console.log("own_celsius=" + Object.prototype.hasOwnProperty.call(t, "celsius"));
console.log("own_backing=" + Object.prototype.hasOwnProperty.call(t, "_c"));
