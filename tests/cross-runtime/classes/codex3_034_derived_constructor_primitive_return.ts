// Cross-runtime: returning a primitive from a derived constructor is a TypeError.
class Base {}
class Child extends Base {
  constructor() { return 3 as any; }
}
let threw = false;
try { new Child(); } catch (e) { threw = e instanceof TypeError; }
console.log(threw);

