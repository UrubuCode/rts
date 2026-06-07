const a = [4, 2, 6, 1, 3, 5];
console.log(a.slice().sort(() => NaN).join(","));
console.log(a.slice().sort((x, y) => (x === 1 ? NaN : x - y)).join(","));
const b = ["b", "a", "c"];
console.log(b.slice().sort((x, y) => undefined as any).join(","));
