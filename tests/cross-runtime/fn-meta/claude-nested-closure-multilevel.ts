function outer(a: number) {
  return function middle(b: number) {
    return function inner(c: number): number {
      return a + b + c;
    };
  };
}
console.log(outer(1)(2)(3));
console.log(outer(10)(20)(30));
const m = outer(100);
console.log(m(5)(1));
console.log(m(6)(2));