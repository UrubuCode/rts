const both = {
  toString() { return "STR"; },
  valueOf() { return 7; },
};
console.log("" + both);
console.log(`${both}`);
console.log(String(both));
console.log(both + "");
console.log(both + 1);
console.log([both].join("-"));
console.log(`${[both, both]}`);
const onlyValue = { valueOf() { return 99; } };
console.log("" + onlyValue);
console.log(`${onlyValue}`);
console.log(String(onlyValue));
console.log([onlyValue].join(""));