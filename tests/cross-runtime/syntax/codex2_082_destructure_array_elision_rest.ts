// Cross-runtime: array elisions consume iterator positions before rest.
const input = [10, 20, 30, 40, 50];
const [first, , third, ...tail] = input;
console.log(first, third, tail.join(","));
console.log(input.length);

