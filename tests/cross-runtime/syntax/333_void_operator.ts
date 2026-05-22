// Cross-runtime: void operator always returns undefined.
console.log(void 0);
console.log(void "anything");
console.log(void (1 + 2));
console.log(void undefined);
console.log(typeof void 0);

// common bundler pattern: void 0 as undefined substitute
const undef = void 0;
console.log(undef === undefined);

// void in arrow to suppress return (bundler output pattern)
const arr = [1, 2, 3];
arr.forEach(x => void console.log(x * 2));
