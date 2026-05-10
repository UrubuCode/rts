// Cross-runtime: richer but basic-safe JSON stringify/parse coverage.
const payload = {
  name: "rts",
  nums: [1, 2, 3],
  nested: { count: 1, text: "line\nbreak" },
};

console.log("plain=" + JSON.stringify(payload));
console.log("arr=" + JSON.stringify([{ id: 1 }, { id: 2 }]));
console.log("quoted=" + JSON.stringify('say "hi"'));
console.log("pretty=" + JSON.stringify({ a: 1, b: [2, 3] }, null, 2).split("\n").join("|"));

const parsed = JSON.parse('{"count":4,"items":[1,2,3],"label":"go","meta":{"x":1}}');
console.log("parsed_count=" + parsed.count);
console.log("parsed_label=" + parsed.label);
console.log("parsed_meta=" + parsed.meta.x);
