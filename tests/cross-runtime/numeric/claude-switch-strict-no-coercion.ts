function classify(x: any): string {
  switch (x) {
    case 1: return "num one";
    case "1": return "str one";
    case true: return "bool true";
    case null: return "null";
    case undefined: return "undefined";
    default: return "other";
  }
}
console.log(classify(1));
console.log(classify("1"));
console.log(classify(true));
console.log(classify(1 === 1));
console.log(classify(null));
console.log(classify(undefined));
console.log(classify(1 + 0));
console.log(classify("1" + ""));
console.log(classify(2));
