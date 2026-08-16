// Cross-runtime: toJSON runs before the replacer sees a property value.
const seen: string[] = [];
const value = {
  item: {
    toJSON(key: string) { seen.push("toJSON:" + key); return { x: 3 }; },
  },
};
const out = JSON.stringify(value, (key, v) => {
  if (key === "item") seen.push("replacer:" + JSON.stringify(v));
  return v;
});
console.log(out);
console.log(seen.join("|"));

