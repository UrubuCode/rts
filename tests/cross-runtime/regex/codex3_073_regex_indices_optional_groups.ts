// Cross-runtime: d-flag indices represent unmatched optional captures as undefined.
const re = /(?<a>a)(?<b>b)?/d;
const first = re.exec("ab")!;
const second = re.exec("a")!;
console.log(JSON.stringify(first.indices));
console.log(JSON.stringify(first.indices!.groups));
console.log(JSON.stringify(second.indices));
console.log(second.indices!.groups!.b);

