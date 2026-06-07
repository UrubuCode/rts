const a = [1, "1", 2, "2", true, false, null, undefined];
console.log(a.indexOf(1));
console.log(a.indexOf("1"));
console.log(a.includes(true));
console.log(a.indexOf(true as any));
console.log(a.indexOf(null));
console.log(a.indexOf(undefined));
console.log(a.includes(0 as any));
console.log(a.includes("" as any));
