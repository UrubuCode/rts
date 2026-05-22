// Cross-runtime: Function.bind
function multiply(this: any, a: number, b: number) {
  return a * b * (this.factor || 1);
}

const obj = { factor: 2 };
const bound = multiply.bind(obj);

console.log("bound=" + bound(3, 4));
console.log("original=" + multiply.call({ factor: 1 }, 3, 4));

// Partial application
const double = multiply.bind({ factor: 1 }, 2);
console.log("partial=" + double(5));

// Bind twice
const bound2 = bound.bind({ factor: 10 });
console.log("rebind=" + bound2(3, 4));
