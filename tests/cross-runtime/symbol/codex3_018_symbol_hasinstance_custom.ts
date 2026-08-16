// Cross-runtime: custom Symbol.hasInstance controls instanceof results.
class Even {
  static [Symbol.hasInstance](value: any) {
    return typeof value === "number" && value % 2 === 0;
  }
}
console.log(2 instanceof Even, 3 instanceof Even, new Number(2) instanceof Even);
console.log({} instanceof Even);

