// Cross-runtime: custom Symbol.hasInstance result coercion.
class Even {
  static [Symbol.hasInstance](x: any) {
    return x && x.value % 2 === 0 ? 1 : 0;
  }
}

console.log(({ value: 4 } instanceof Even));
console.log(({ value: 5 } instanceof Even));
console.log((null instanceof (Even as any)));
