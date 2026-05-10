// Cross-runtime compatibility: JSON replacer/reviver and omission semantics.
const payload = {
  keep: 1,
  drop: undefined as undefined | number,
  nested: {
    active: true,
    skip: undefined as undefined | string,
  },
  list: [1, undefined, 3],
};

console.log("plain=" + JSON.stringify(payload));
console.log(
  "replacer_fn=" +
    JSON.stringify(payload, (key: string, value: unknown) => {
      if (key === "keep") return (value as number) + 9;
      if (key === "active") return value ? "yes" : "no";
      return value;
    }),
);
console.log("replacer_arr=" + JSON.stringify(payload, ["nested", "active", "list"]));

const parsed = JSON.parse(
  '{"count":4,"items":[1,2,3],"label":"go"}',
  (key: string, value: unknown) => {
    if (key === "count") return (value as number) * 2;
    if (key === "label") return String(value).toUpperCase();
    return value;
  },
);

console.log("parsed_count=" + parsed.count);
console.log("parsed_items=" + parsed.items.join(","));
console.log("parsed_label=" + parsed.label);
