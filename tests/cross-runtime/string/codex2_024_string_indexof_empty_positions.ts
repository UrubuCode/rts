// Cross-runtime: empty-needle searches clamp start positions at both ends.
const s = "abc";
console.log([-5, 0, 2, 9].map((p) => s.indexOf("", p)).join(","));
console.log([-5, 0, 2, 9].map((p) => s.lastIndexOf("", p)).join(","));

