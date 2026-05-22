// Cross-runtime future coverage: deeper class behavior.
class BaseCounter {
  label: string;
  _value: number;

  constructor(label: string, value: number) {
    this.label = label;
    this._value = value;
  }

  get value(): number {
    return this._value;
  }

  set value(next: number) {
    this._value = next;
  }

  bump(step: number): number {
    this._value += step;
    return this._value;
  }

  static seed(label: string): BaseCounter {
    return new BaseCounter(label, 1);
  }
}

class DoubleCounter extends BaseCounter {
  bump(step: number): number {
    return super.bump(step * 2);
  }

  info(): string {
    return this.label + ":" + this.value;
  }
}

const a = BaseCounter.seed("alpha");
console.log("a0=" + a.value);
console.log("a1=" + a.bump(4));
a.value = 12;
console.log("a2=" + a.value);

const b = new DoubleCounter("beta", 5);
console.log("b0=" + b.bump(3));
console.log("b1=" + b.info());
console.log("is_base=" + (b instanceof BaseCounter));
console.log("is_double=" + (b instanceof DoubleCounter));
