// Cross-runtime: safer subset of nullish coalescing + optional chaining.
type User = {
  name?: string;
  tags?: string[];
};

const missing: User | null = null;
const partial: User = {};
const rich: User = { name: "Ada", tags: ["core", "fast"] };

console.log("null_name=" + (missing?.name ?? "anon"));
console.log("missing_name=" + (partial.name ?? "anon"));
console.log("rich_name=" + (rich.name ?? "anon"));
console.log("tag0=" + (rich.tags?.[0] ?? "none"));
console.log("tag9=" + (rich.tags?.[9] ?? "none"));
console.log("missing_tag=" + (partial.tags?.[0] ?? "none"));
console.log("null_chain=" + (missing?.tags?.[0] ?? "none"));
console.log("nullish_a=" + (null ?? "fallback"));
console.log("chain_mix=" + ((missing?.name ?? "x") + ":" + (rich.tags?.[1] ?? "y")));
