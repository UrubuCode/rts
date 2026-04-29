import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Computed property keys
const key1 = "foo";
const key2 = "bar";
const obj1 = { [key1]: 42, [key2]: 99 };
print(`${obj1.foo} ${obj1.bar}`);

// Computed key from expression
const prefix = "on";
const handlers = {
  [prefix + "Click"]: function() { return "clicked"; },
  [prefix + "Hover"]: function() { return "hovered"; },
};
print(`${handlers.onClick()}`);
print(`${handlers.onHover()}`);

// Method shorthand in object literal
const calculator = {
  value: 0,
  add(n: number) { this.value += n; return this; },
  sub(n: number) { this.value -= n; return this; },
  result() { return this.value; },
};
calculator.add(10).add(5).sub(3);
print(`${calculator.result()}`);

// Getter/setter in object literal
const temperature = {
  _celsius: 0,
  get fahrenheit() { return this._celsius * 9 / 5 + 32; },
  set fahrenheit(f: number) { this._celsius = (f - 32) * 5 / 9; },
};
temperature.fahrenheit = 212;
print(`${temperature._celsius}`);
print(`${temperature.fahrenheit}`);

describe("object_computed_methods", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe("42 99\nclicked\nhovered\n12\n100\n212\n"));
});
