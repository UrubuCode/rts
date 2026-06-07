// Cross-runtime: destructuring with computed names, defaults, and rest.
const key = "left";
const src: any = { left: 3, right: undefined, extra: 9, nested: { value: 5 } };
const { [key]: l, right = l * 4, nested: { value: v = 0 }, ...rest } = src;

console.log("l=" + l);
console.log("right=" + right);
console.log("v=" + v);
console.log("rest=" + Object.keys(rest).sort().join(",") + ":" + JSON.stringify(rest));

const arr = [1, , 3, 4] as any[];
const [first, second = first + 10, ...tail] = arr;
console.log("array=" + first + "," + second + "," + tail.join("|") + ",len=" + tail.length);
