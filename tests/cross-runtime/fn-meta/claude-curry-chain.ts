function adder(a: number) {
  return function (b: number) {
    return function (c: number) {
      return function (d: number): number {
        return a + b + c + d;
      };
    };
  };
}
console.log(adder(1)(2)(3)(4));
const step1 = adder(10);
const step2 = step1(20);
const step3 = step2(30);
console.log(step3(40));
console.log(step2(5)(6));