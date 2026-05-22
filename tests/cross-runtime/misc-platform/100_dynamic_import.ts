// Cross-runtime compatibility: dynamic import and module caching.
async function main() {
  const first = await import("./support/dynamic_mod.mjs");
  const second = await import("./support/dynamic_mod.mjs");

  console.log("named=" + first.namedValue);
  console.log("default=" + first.default("x"));
  console.log("same=" + String(first === second));
  console.log("count=" + first.state.count);
}

main();
