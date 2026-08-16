// Cross-runtime: abstract equality coerces objects against primitives but never two objects.
let calls = 0;
const value = { valueOf() { calls++; return 5; } };
console.log(value == 5, value == "5", value == true);
console.log(calls);
console.log(value == { valueOf() { return 5; } });

