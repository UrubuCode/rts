// Cross-runtime future coverage: closures, var/let and arrow this capture.
function makeCounter(start: number) {
  let value = start;
  return function (): number {
    value += 1;
    return value;
  };
}

const c = makeCounter(3);
console.log("counter=" + c() + ":" + c());

const iife = (function (x: number): number {
  return x * 2;
})(6);
console.log("iife=" + iife);

let letOut = "";
for (let i = 0; i < 3; i++) {
  const fn = () => i;
  letOut += fn() + ",";
}
console.log("let=" + letOut);

let varOut = "";
for (var j = 0; j < 3; j++) {
  const fn = () => j;
  varOut += fn() + ",";
}
console.log("var=" + varOut);

function nested(base: number) {
  return function (extra: number): number {
    return base + extra;
  };
}
console.log("nested=" + nested(10)(5));

const obj = {
  value: 7,
  make() {
    return () => this.value + 1;
  },
};
console.log("this_arrow=" + obj.make()());
