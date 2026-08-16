// Cross-runtime: the extends expression evaluates once when defining the class.
let calls = 0;
class Root { value() { return 6; } }
function base() { calls++; return Root; }
class Child extends base() {}
console.log(new Child().value(), calls);
console.log(Object.getPrototypeOf(Child) === Root);

